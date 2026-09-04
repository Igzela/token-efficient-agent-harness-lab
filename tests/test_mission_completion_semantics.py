"""Regression tests for Mission completion semantics and research acceptance ledgers.

Enforces:
1. Final preplanned Stage merges with unresolved obligations -> stays RUNNING / RESEARCH_PENDING.
2. All Stage PRs complete but RWE unresolved -> not COMPLETE.
3. Operational BLOCKED_AUTHORITY rejected as scientific terminal disposition.
4. Preceding-gate dependency semantics (halting upstream allows NOT_JUSTIFIED_BY_PRECEDING_GATE).
5. Dynamically generated follow-up Stage for unresolved eligible obligations.
6. Service restart restores exact acceptance ledger from journal replay.
7. Duplicate evidence/disposition is idempotent.
8. Contradictory evidence fails closed.
9. Mission completes only after every required obligation has accepted terminal evidence.
10. Simple ordinary maintenance missions without acceptance ledger retain bounded completion.
"""

from __future__ import annotations

from dataclasses import replace
import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
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
            owner_authenticator=type("Auth", (), {"verify": lambda *_args: True})(),
            mission=proposal,
        )

    def test_01_final_preplanned_stage_merges_with_unresolved_obligations_stays_running(self):
        """Test 1: Final preplanned Stage merges but unresolved obligations remain -> stays RUNNING."""
        ob1 = contract.AcceptanceObligation(
            obligation_id="common_rwe_evidence_basis",
            description="Validate frozen RWE evidence basis.",
            category="basis",
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
            github_writer=self.github_writer,
            repo_path=self.repo_dir,
            control_state=type("FakeControl", (), {"emergency_stop_active": lambda *a, **kw: False})(),
        )

        # Stage 1 is the sole preplanned stage (index 1 of 1)
        # Step plans stage 1
        res1 = srv.step()
        self.assertEqual(res1["status"], "STAGE_PLANNED")
        stage_id = res1["stage_id"]

        # Simulate stage execution and merge readback
        new_commit = subprocess.run(
            ["git", "commit", "--allow-empty", "-m", "Stage 1 merge commit"],
            cwd=self.repo_dir,
            check=True,
            capture_output=True,
        )
        merged_sha = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=self.repo_dir,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        self.github_writer.remote_main_sha = merged_sha

        # Readback on final stage
        with patch("subprocess.run") as git_run:
            git_run.return_value = MagicMock(returncode=0, stdout=merged_sha + "\n", stderr="")
            readback = srv.post_merge_readback(
                stage_id=stage_id,
                pr_number=601,
                expected_head_sha=merged_sha,
                is_final_stage=True,
            )
        self.assertEqual(readback["status"], "VERIFIED")
        # Mission must NOT be COMPLETE because ob1 is unresolved!
        self.assertEqual(readback["mission_state"], "RUNNING")
        self.assertEqual(srv.mission.state, "RUNNING")

        # Further advance returns dynamic stage or RESEARCH_PENDING, not COMPLETE
        res_next = srv.step()
        self.assertIn(res_next["status"], {"STAGE_PLANNED", "RESEARCH_PENDING"})
        self.assertNotEqual(res_next["status"], "COMPLETE")

    def test_02_all_stage_prs_complete_but_rwe_unresolved_not_complete(self):
        """Test 2: All Stage PRs complete but RWE unresolved -> not COMPLETE."""
        proposal = contract.build_research_successor_mission(base_sha=self.base_sha)
        mission = contract.activate_current_mission(
            repository=proposal.repository_identity.repository,
            base_sha=proposal.repository_identity.base_sha,
            branch=proposal.repository_identity.branch,
            source_ref=proposal.repository_identity.source_ref,
            source_sha256=proposal.repository_identity.source_sha256,
            proposal_sha256=proposal.proposal_sha256,
            owner_approval=contract.OwnerApproval(
                "github:Igzela", proposal.proposal_sha256, "fixture-approval", "2026-09-04T00:00:00Z"
            ),
            owner_authenticator=type("Auth", (), {"verify": lambda *_args: True})(),
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
        self.assertIsNotNone(srv.mission.acceptance_ledger)
        self.assertFalse(srv.mission.acceptance_ledger.is_terminal())

        # Calling complete_mission_if_eligible must refuse completion
        res = srv.complete_mission_if_eligible()
        self.assertEqual(res["status"], "RESEARCH_PENDING")
        self.assertEqual(srv.mission.state, "RUNNING")

    def test_03_operational_blocked_authority_rejected_as_scientific_terminal(self):
        """Test 3: Operational BLOCKED_AUTHORITY rejected as scientific terminal disposition."""
        ob = contract.AcceptanceObligation(
            obligation_id="mx1_c1_1x2x1",
            description="Matrix rung evaluation",
            category="evaluation",
            dependencies=(),
            required_paths=("README.md",),
        )
        ledger = contract.MissionAcceptanceLedger((ob,))
        ledger.validate()

        # Rejection in ledger
        with self.assertRaises(contract.MissionContractError) as ctx:
            ledger.disposition_obligation("mx1_c1_1x2x1", "BLOCKED_AUTHORITY", {"reason": "needs auth"})
        self.assertEqual(str(ctx.exception), "operational_state_not_scientific_terminal")

        # Rejection in journal
        with self.assertRaises(JournalError):
            self.journal.record_obligation_disposition(
                "M1", "mx1_c1_1x2x1", "BLOCKED_AUTHORITY", {"reason": "needs auth"}
            )

    def test_04_preceding_gate_dependency_semantics(self):
        """Test 4: Halting upstream gate allows NOT_JUSTIFIED_BY_PRECEDING_GATE; unresolved/complete fails."""
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
        ledger.validate()

        # Fails when upstream is unresolved
        with self.assertRaises(contract.MissionContractError) as ctx:
            ledger.disposition_obligation("rung2", "NOT_JUSTIFIED_BY_PRECEDING_GATE", {"evidence": "skip"})
        self.assertEqual(str(ctx.exception), "not_justified_requires_halting_preceding_gate")

        # Upstream completes with non-halting COMPLETE
        ledger_comp = ledger.disposition_obligation("rung1", "COMPLETE", {"passed": True})
        # Fails when upstream is COMPLETE
        with self.assertRaises(contract.MissionContractError) as ctx:
            ledger_comp.disposition_obligation("rung2", "NOT_JUSTIFIED_BY_PRECEDING_GATE", {"evidence": "skip"})
        self.assertEqual(str(ctx.exception), "not_justified_requires_halting_preceding_gate")

        # Upstream completes with halting INCOMPARABLE
        ledger_halt = ledger.disposition_obligation("rung1", "INCOMPARABLE", {"reason": "evidence_missing"})
        # Now downstream CAN be NOT_JUSTIFIED_BY_PRECEDING_GATE
        ledger_halt2 = ledger_halt.disposition_obligation(
            "rung2", "NOT_JUSTIFIED_BY_PRECEDING_GATE", {"upstream": "rung1", "reason": "incomparable"}
        )
        self.assertEqual(ledger_halt2.get("rung2").disposition, "NOT_JUSTIFIED_BY_PRECEDING_GATE")

    def test_05_dynamically_generated_follow_up_stage_satisfies_obligation(self):
        """Test 5: Dynamically generated follow-up Stage satisfies an unresolved obligation."""
        ob1 = contract.AcceptanceObligation(
            obligation_id="common_rwe_evidence_basis",
            description="Validate frozen RWE evidence basis.",
            category="basis",
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

        plan = srv._next_dynamic_stage_plan(mission, index=1, eligible=(ob1,))
        self.assertIsNotNone(plan)
        stage, cards, total = plan
        self.assertEqual(total, 2)
        self.assertEqual(len(cards), 1)
        self.assertEqual(cards[0].allowed_paths, ("README.md",))
        self.assertIn("common_rwe_evidence_basis", stage.objective)

    def test_06_restart_restores_exact_acceptance_ledger_from_journal_replay(self):
        """Test 6: Restart restores exact acceptance ledger from journal replay."""
        ob1 = contract.AcceptanceObligation(
            obligation_id="node_a",
            description="Node A",
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
        evidence = {"hash": "abc1234", "metrics": {"cost": 0}}
        srv1.disposition_mission_obligation("node_a", "COMPLETE", evidence)

        # Simulate restart with fresh service instance connected to same SQLite journal
        srv2 = service.StewardService(
            mission_id=mission.mission_id,
            journal=self.journal,
            github=self.github_writer,
            repo_path=self.repo_dir,
        )
        self.assertIsNotNone(srv2.mission)
        self.assertIsNotNone(srv2.mission.acceptance_ledger)
        node_a = srv2.mission.acceptance_ledger.get("node_a")
        self.assertIsNotNone(node_a)
        self.assertEqual(node_a.disposition, "COMPLETE")
        self.assertEqual(node_a.evidence, evidence)

    def test_07_duplicate_evidence_disposition_is_idempotent(self):
        """Test 7: Duplicate evidence/disposition is idempotent."""
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
        ev = {"canonical_hash": "sha256:1111"}
        evt1 = srv.disposition_mission_obligation("node_idempotent", "COMPLETE", ev)
        evt2 = srv.disposition_mission_obligation("node_idempotent", "COMPLETE", ev)
        self.assertEqual(evt1.seq, evt2.seq)
        self.assertEqual(evt1.sha256, evt2.sha256)

    def test_08_contradictory_evidence_fails_closed(self):
        """Test 8: Contradictory evidence fails closed."""
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
        srv.disposition_mission_obligation("node_contradict", "COMPLETE", {"result": "pass"})

        # Different disposition fails
        with self.assertRaises(contract.MissionContractError) as ctx:
            srv.disposition_mission_obligation("node_contradict", "NO_GO", {"result": "pass"})
        self.assertEqual(str(ctx.exception), "contradictory_obligation_evidence")

        # Different evidence fails
        with self.assertRaises(contract.MissionContractError) as ctx:
            srv.disposition_mission_obligation("node_contradict", "COMPLETE", {"result": "different"})
        self.assertEqual(str(ctx.exception), "contradictory_obligation_evidence")

    def test_09_mission_completes_only_after_every_required_obligation_has_accepted_terminal_evidence(self):
        """Test 9: Mission completes only after every required obligation has accepted terminal evidence."""
        ob1 = contract.AcceptanceObligation(
            obligation_id="ob1",
            description="First obligation",
            category="gate",
            dependencies=(),
            required_paths=("README.md",),
        )
        ob2 = contract.AcceptanceObligation(
            obligation_id="ob2",
            description="Second obligation",
            category="evaluation",
            dependencies=("ob1",),
            required_paths=("README.md",),
        )
        mission = self._make_mission_with_ledger((ob1, ob2), allowed_paths=("README.md",))
        self.journal.record_mission_activation(
            mission.mission_id, mission.proposal_sha256, mission.to_wire()
        )
        srv = service.StewardService(
            mission_id=mission.mission_id,
            journal=self.journal,
            github=self.github_writer,
            repo_path=self.repo_dir,
        )

        # Neither dispositioned
        self.assertEqual(srv.complete_mission_if_eligible()["status"], "RESEARCH_PENDING")

        # 1 of 2 dispositioned
        srv.disposition_mission_obligation("ob1", "COMPLETE", {"evidence": "ob1_done"})
        self.assertEqual(srv.complete_mission_if_eligible()["status"], "RESEARCH_PENDING")

        # 2 of 2 dispositioned
        srv.disposition_mission_obligation("ob2", "COMPLETE", {"evidence": "ob2_done"})
        res = srv.complete_mission_if_eligible()
        self.assertEqual(res["status"], "COMPLETE")
        self.assertEqual(srv.mission.state, "COMPLETE")

        # Verified in journal projection
        proj = self.journal.projection(mission_id=mission.mission_id)
        self.assertIn("ob1", proj["obligations"])
        self.assertIn("ob2", proj["obligations"])
        self.assertEqual(proj["obligations"]["ob1"]["disposition"], "COMPLETE")
        self.assertEqual(proj["obligations"]["ob2"]["disposition"], "COMPLETE")

    def test_10_simple_ordinary_maintenance_mission_without_acceptance_ledger_retains_bounded_completion(self):
        """Test 10: Simple ordinary maintenance missions without acceptance ledger retain bounded completion."""
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

        # With no stages pending, simple mission completes normally
        res = srv.complete_mission_if_eligible()
        self.assertEqual(res["status"], "COMPLETE")
        self.assertEqual(srv.mission.state, "COMPLETE")


if __name__ == "__main__":
    unittest.main()
