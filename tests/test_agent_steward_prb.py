"""PR-B Unit and Integration Tests for Single Canonical Merge Owner & Mutation Guards."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts" / "agent-control"))

import steward_github as gh
from steward_github import (
    FakeGitHubReader,
    FakeGitHubWriter,
    GhGitHubWriter,
    GitHubMutationError,
    GitHubPreflightError,
    GitHubFactsError,
    GitHubReadError,
)


class TestAutonomousStewardPRB(unittest.TestCase):
    def setUp(self):
        self.repo = "Igzela/token-efficient-agent-harness-lab"
        self.head_sha = "a" * 40
        self.base_sha = "b" * 40

    def test_fake_writer_contract_parity(self):
        """Verify FakeGitHubWriter provides complete fidelity to GitHubWriter protocol."""
        writer = FakeGitHubWriter(initial_pr_number=201)

        # 1. Create Stage Draft PR
        pr = writer.create_or_update_stage_pr(
            stage_id="stage-1",
            mission_id="mission-1",
            branch="stage/stage-1",
            expected_sha=self.head_sha,
            base_sha=self.base_sha,
            title="Stage 1 PR",
            body="Stage 1 PR description",
            repository=self.repo,
        )
        self.assertEqual(pr["pr_number"], 201)
        self.assertTrue(pr["draft"])
        self.assertFalse(pr["merged"])
        self.assertEqual(pr["head_sha"], self.head_sha)

        # 2. Mark Ready fails on head mismatch
        with self.assertRaises(GitHubMutationError):
            writer.mark_ready(self.repo, 201, "f" * 40)

        # 3. Mark Ready succeeds on exact head match
        ready = writer.mark_ready(self.repo, 201, self.head_sha)
        self.assertTrue(ready)
        facts = writer.fetch_stage_pr(self.repo, 201)
        self.assertFalse(facts["draft"])

        # 4. Guarded merge fails on head mismatch
        with self.assertRaises(GitHubMutationError):
            writer.guarded_merge(self.repo, 201, "f" * 40)

        # 5. Guarded merge succeeds on exact head match
        merge_res = writer.guarded_merge(self.repo, 201, self.head_sha)
        self.assertTrue(merge_res["merged"])
        self.assertEqual(merge_res["head_sha"], self.head_sha)

        # 6. Post-merge readback returns authoritative SHA
        readback = writer.post_merge_readback(self.repo, 201, self.head_sha)
        self.assertEqual(readback["status"], "VERIFIED")
        self.assertEqual(readback["accepted_main_sha"], self.head_sha)

    def test_fake_recovery_authorization_requires_complete_identity(self):
        writer = FakeGitHubWriter(initial_pr_number=202)
        marker = {
            "mission_id": "MISSION-ORPHAN",
            "proposal_sha256": "c" * 64,
            "stage_id": "stage-orphan",
            "repository": self.repo,
            "control_issue_number": 208,
            "pr_number": 202,
            "base_sha": self.base_sha,
            "head_sha": self.head_sha,
            "workflow_file": "agent-merge.yml",
            "ref": "main",
            "dispatch_id": "d" * 64,
            "authorization": "ORPHAN_DISPATCH_RECOVERY",
            "action": "QUARANTINE_EXACT_PR",
            "authorization_id": "owner-recovery-fake",
            "owner_identity": "github:Igzela",
        }
        writer.merge_dispatch_resolutions.append(marker)
        common = dict(
            repository=self.repo,
            control_issue_number=208,
            mission_id="MISSION-ORPHAN",
            proposal_sha256="c" * 64,
            stage_id="stage-orphan",
            pr_number=202,
            expected_base_sha=self.base_sha,
            expected_head_sha=self.head_sha,
            workflow_file="agent-merge.yml",
            dispatch_id="d" * 64,
            owner_identity="github:Igzela",
        )
        self.assertIsNotNone(writer.read_orphan_dispatch_recovery_authorization(**common))
        self.assertIsNone(
            writer.read_orphan_dispatch_recovery_authorization(
                **{**common, "expected_head_sha": "e" * 40}
            )
        )
        self.assertIsNone(
            writer.read_orphan_dispatch_recovery_authorization(
                **{**common, "owner_identity": "github:attacker"}
            )
        )

    def test_mark_ready_exact_head_guard(self):
        """Verify GhGitHubWriter.mark_ready re-reads live head before mutating PR state."""
        writer = GhGitHubWriter(timeout_seconds=10)

        # Mock reader to return mismatched head
        mock_facts = {
            "repository": self.repo,
            "pr_number": 301,
            "state": "OPEN",
            "draft": True,
            "merged": False,
            "base_sha": self.base_sha,
            "head_sha": "9" * 40,  # Different from expected head
            "ci_state": "PASS",
            "review_state": "PASS",
            "base_branch": "main",
            "head_branch": "stage-branch",
        }

        with patch.object(writer.reader, "fetch_stage_pr", return_value=mock_facts):
            with self.assertRaisesRegex(GitHubMutationError, "exact_head_mismatch_before_mark_ready"):
                writer.mark_ready(self.repo, 301, self.head_sha)

    def test_review_receipt_binding_drift_fails_before_any_mutation(self):
        writer = GhGitHubWriter(timeout_seconds=10)
        facts = {
            "repository": self.repo,
            "pr_number": 305,
            "state": "OPEN",
            "draft": True,
            "merged": False,
            "base_sha": "c" * 40,
            "head_sha": self.head_sha,
            "ci_state": "PENDING",
            "review_state": "PENDING",
            "base_branch": "main",
            "head_branch": "stage-branch",
        }
        with (
            patch.object(writer, "fetch_stage_pr", return_value=facts),
            patch("subprocess.run") as mutation,
        ):
            with self.assertRaisesRegex(
                GitHubPreflightError, "review_receipt_exact_binding_mismatch"
            ):
                writer.publish_exact_head_review(
                    self.repo,
                    305,
                    self.head_sha,
                    base_sha=self.base_sha,
                    reviewer_session_id="review-session",
                    implementation_session_id="implementation-session",
                    reviewed_range_sha256="d" * 64,
                    review_receipt_sha256="e" * 64,
                )
        mutation.assert_not_called()

    def test_guarded_merge_dispatches_workflow_and_verifies_result(self):
        """Verify guarded_merge delegates strictly to agent-merge.yml and reads back merge proof."""
        writer = GhGitHubWriter(timeout_seconds=5)
        dispatch_id = gh.merge_dispatch_identity(
            self.repo,
            302,
            self.base_sha,
            self.head_sha,
            intent_key="compatibility-call",
        )["dispatch_id"]

        mock_facts_before = {
            "repository": self.repo,
            "pr_number": 302,
            "state": "OPEN",
            "draft": False,
            "merged": False,
            "base_sha": self.base_sha,
            "head_sha": self.head_sha,
            "ci_state": "PASS",
            "review_state": "PASS",
            "base_branch": "main",
            "head_branch": "stage-branch",
        }

        # Dispatch command returns a durable run identity.  The adapter does
        # not infer a merge outcome from the dispatch response.
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(
                returncode=0,
                stdout=json.dumps({
                    "workflow_run_id": 901,
                    "run_url": "https://api.github.com/repos/example/actions/runs/901",
                    "html_url": "https://github.com/example/actions/runs/901",
                }),
                stderr="",
            )
            with patch.object(writer, "fetch_stage_pr", return_value=mock_facts_before):
                result = writer.guarded_merge(self.repo, 302, self.head_sha, workflow_file="agent-merge.yml")
                self.assertEqual(result["status"], "DISPATCHED")
                self.assertFalse(result["merged"])
                self.assertEqual(result["head_sha"], self.head_sha)
                self.assertEqual(result["workflow_run_id"], 901)

                # Verify subprocess used the REST dispatch endpoint, NOT gh pr merge.
                call_args = mock_run.call_args[0][0]
                self.assertEqual(call_args[:4], ["gh", "api", "--method", "POST"])
                self.assertIn("actions/workflows/agent-merge.yml/dispatches", call_args[4])
                self.assertNotIn("pr", call_args[:3])
                self.assertEqual(
                    json.loads(mock_run.call_args.kwargs["input"]),
                    {
                        "ref": "main",
                        "return_run_details": True,
                        "inputs": {
                            "pr_number": "302",
                            "head_sha": self.head_sha,
                            "dispatch_id": dispatch_id,
                        },
                    },
                )

    def test_guarded_merge_fail_closed_when_dispatch_response_is_lost(self):
        """A possibly accepted dispatch with a lost response is never replayed."""
        writer = GhGitHubWriter(timeout_seconds=1)

        mock_facts_unmerged = {
            "repository": self.repo,
            "pr_number": 303,
            "state": "OPEN",
            "draft": False,
            "merged": False,
            "base_sha": self.base_sha,
            "head_sha": self.head_sha,
            "ci_state": "PASS",
            "review_state": "PASS",
            "base_branch": "main",
            "head_branch": "stage-branch",
        }

        with patch(
            "subprocess.run",
            side_effect=subprocess.TimeoutExpired(cmd="gh api", timeout=1),
        ) as command:
            with patch.object(writer, "fetch_stage_pr", return_value=mock_facts_unmerged):
                with self.assertRaisesRegex(GitHubMutationError, "merge_outcome_unknown"):
                    writer.guarded_merge(self.repo, 303, self.head_sha, timeout_seconds=1)
        self.assertEqual(command.call_count, 1)

    def test_canonical_merge_workflow_records_dispatch_identity(self):
        """The workflow must retain the third exact binding for reconciliation."""

        workflow = (
            Path(__file__).resolve().parents[1]
            / ".github"
            / "workflows"
            / "agent-merge.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("dispatch_id:", workflow)
        self.assertIn("required: true", workflow)
        self.assertIn("DISPATCH_ID: ${{ inputs.dispatch_id }}", workflow)
        self.assertIn("DISPATCH_ID: %s", workflow)

    def test_reconcile_merge_dispatch_binds_terminal_failure_to_exact_head(self):
        """Read-only workflow logs can prove a failed dispatch without rerun."""

        writer = GhGitHubWriter(timeout_seconds=5)
        dispatch_id = "c" * 64
        run_list = json.dumps(
            [
                {"workflow_runs": [{
                    "id": 777,
                    "status": "completed",
                    "conclusion": "failure",
                    "created_at": "2026-08-31T00:00:00Z",
                    "head_sha": self.base_sha,
                    "head_branch": "main",
                    "event": "workflow_dispatch",
                    "path": ".github/workflows/agent-merge.yml@main",
                }]}
            ]
        )
        run_log = (
            "merge Validate live PR\n"
            f"  PR_NUMBER: 17\n  EXPECTED_HEAD: {self.head_sha}\n"
            f"  DISPATCH_ID: {dispatch_id}\n"
            "merge Merge one exact head\n"
        )
        with patch("subprocess.run") as run:
            run.side_effect = [
                MagicMock(returncode=0, stdout=run_list, stderr=""),
                MagicMock(returncode=0, stdout=run_log, stderr=""),
            ]
            result = writer.reconcile_merge_dispatch(
                self.repo,
                17,
                self.head_sha,
                workflow_file="agent-merge.yml",
                dispatch_id=dispatch_id,
            )
        self.assertEqual(result["status"], "REJECTED")
        self.assertEqual(result["run_ids"], [777])
        self.assertIn("--log", run.call_args_list[1].args[0])
        self.assertNotIn("--log-failed", run.call_args_list[1].args[0])

    def test_reconcile_merge_dispatch_consumes_durable_run_id(self):
        """A REST dispatch run ID still requires exact terminal log binding."""

        writer = GhGitHubWriter(timeout_seconds=5)
        dispatch_id = "d" * 64
        run = {
            "id": 790,
            "status": "completed",
            "conclusion": "failure",
            "created_at": "2026-08-31T00:00:00Z",
            "head_sha": self.base_sha,
            "head_branch": "main",
            "event": "workflow_dispatch",
            "path": ".github/workflows/agent-merge.yml@main",
        }
        run_log = (
            f"merge PR_NUMBER: 17\nEXPECTED_HEAD: {self.head_sha}\n"
            f"DISPATCH_ID: {dispatch_id}\n"
        )
        with patch("subprocess.run") as command:
            command.side_effect = [
                MagicMock(returncode=0, stdout=json.dumps(run), stderr=""),
                MagicMock(returncode=0, stdout=run_log, stderr=""),
            ]
            result = writer.reconcile_merge_dispatch(
                self.repo,
                17,
                self.head_sha,
                expected_base_sha=self.base_sha,
                dispatch_id=dispatch_id,
                workflow_run_id=790,
            )
        self.assertEqual(result["status"], "REJECTED")
        self.assertEqual(result["run_ids"], [790])
        self.assertEqual(command.call_args_list[0].args[0][-1], f"repos/{self.repo}/actions/runs/790")
        self.assertIn("--log", command.call_args_list[1].args[0])

    def test_reconcile_known_run_rejects_missing_input_binding(self):
        """A durable run ID without exact PR/head log evidence stays unknown."""

        writer = GhGitHubWriter(timeout_seconds=5)
        dispatch_id = "e" * 64
        run = {
            "id": 791,
            "status": "completed",
            "conclusion": "failure",
            "created_at": "2026-08-31T00:00:00Z",
            "head_sha": self.base_sha,
            "head_branch": "main",
            "event": "workflow_dispatch",
            "path": ".github/workflows/agent-merge.yml@main",
        }
        with patch("subprocess.run") as command:
            command.side_effect = [
                MagicMock(returncode=0, stdout=json.dumps(run), stderr=""),
                MagicMock(
                    returncode=0,
                    stdout=(
                        f"merge PR_NUMBER: 18\nEXPECTED_HEAD: {self.head_sha}\n"
                        f"DISPATCH_ID: {dispatch_id}\n"
                    ),
                    stderr="",
                ),
            ]
            result = writer.reconcile_merge_dispatch(
                self.repo,
                17,
                self.head_sha,
                expected_base_sha=self.base_sha,
                dispatch_id=dispatch_id,
                workflow_run_id=791,
            )
        self.assertEqual(result["status"], "NOT_PROVEN")
        self.assertTrue(result["binding_mismatch"])
        self.assertEqual(result["run_ids"], [791])

    def test_reconcile_known_run_rejects_dispatch_identity_mismatch(self):
        """A matching PR/head with another dispatch identity stays unknown."""

        writer = GhGitHubWriter(timeout_seconds=5)
        dispatch_id = "a" * 64
        run = {
            "id": 792,
            "status": "completed",
            "conclusion": "failure",
            "created_at": "2026-08-31T00:00:00Z",
            "head_sha": self.base_sha,
            "head_branch": "main",
            "event": "workflow_dispatch",
            "path": ".github/workflows/agent-merge.yml@main",
        }
        with patch("subprocess.run") as command:
            command.side_effect = [
                MagicMock(returncode=0, stdout=json.dumps(run), stderr=""),
                MagicMock(
                    returncode=0,
                    stdout=(
                        f"merge PR_NUMBER: 17\nEXPECTED_HEAD: {self.head_sha}\n"
                        f"DISPATCH_ID: {'b' * 64}\n"
                    ),
                    stderr="",
                ),
            ]
            result = writer.reconcile_merge_dispatch(
                self.repo,
                17,
                self.head_sha,
                expected_base_sha=self.base_sha,
                dispatch_id=dispatch_id,
                workflow_run_id=792,
            )
        self.assertEqual(result["status"], "NOT_PROVEN")
        self.assertTrue(result["binding_mismatch"])
        self.assertEqual(result["run_ids"], [792])

    def test_reconcile_dispatch_id_survives_initially_undiscoverable_run(self):
        """Restart may retry read-only discovery, never the old dispatch POST."""

        writer = GhGitHubWriter(timeout_seconds=5)
        dispatch_id = "9" * 64
        run = {
            "id": 793,
            "status": "completed",
            "conclusion": "failure",
            "created_at": "2026-09-01T12:00:00Z",
            "head_sha": self.base_sha,
            "head_branch": "main",
            "event": "workflow_dispatch",
            "path": ".github/workflows/agent-merge.yml@main",
        }
        run_log = (
            f"PR_NUMBER: 17\nEXPECTED_HEAD: {self.head_sha}\n"
            f"DISPATCH_ID: {dispatch_id}\n"
        )
        with patch("subprocess.run") as command:
            command.side_effect = [
                MagicMock(returncode=0, stdout=json.dumps([{"workflow_runs": []}]), stderr=""),
                MagicMock(
                    returncode=0,
                    stdout=json.dumps([{"workflow_runs": [run]}]),
                    stderr="",
                ),
                MagicMock(returncode=0, stdout=run_log, stderr=""),
            ]
            first = writer.reconcile_merge_dispatch(
                self.repo,
                17,
                self.head_sha,
                expected_base_sha=self.base_sha,
                dispatch_id=dispatch_id,
            )
            second = writer.reconcile_merge_dispatch(
                self.repo,
                17,
                self.head_sha,
                expected_base_sha=self.base_sha,
                dispatch_id=dispatch_id,
            )
        self.assertEqual(first["status"], "NOT_PROVEN")
        self.assertEqual(second["status"], "REJECTED")
        self.assertTrue(
            all("--method" not in call.args[0] for call in command.call_args_list)
        )

    def test_reconcile_known_run_rejects_base_binding_mismatch(self):
        """A run from another dispatch base cannot settle this intent."""

        writer = GhGitHubWriter(timeout_seconds=5)
        run = {
            "id": 794,
            "status": "completed",
            "conclusion": "failure",
            "created_at": "2026-09-01T12:00:00Z",
            "head_sha": "f" * 40,
            "head_branch": "main",
            "event": "workflow_dispatch",
            "path": ".github/workflows/agent-merge.yml@main",
        }
        with patch(
            "subprocess.run",
            return_value=MagicMock(returncode=0, stdout=json.dumps(run), stderr=""),
        ) as command:
            result = writer.reconcile_merge_dispatch(
                self.repo,
                17,
                self.head_sha,
                expected_base_sha=self.base_sha,
                dispatch_id="8" * 64,
                workflow_run_id=794,
            )
        self.assertEqual(result["status"], "NOT_PROVEN")
        self.assertTrue(result["binding_mismatch"])
        self.assertEqual(command.call_count, 1)

    def test_orphan_without_run_id_remains_not_proven(self):
        """An empty exact-head run list is not a no-effect proof."""

        writer = GhGitHubWriter(timeout_seconds=5)
        with patch(
            "subprocess.run",
            return_value=MagicMock(
                returncode=0,
                stdout=json.dumps([{"total_count": 0, "workflow_runs": []}]),
                stderr="",
            ),
        ) as command:
            result = writer.reconcile_merge_dispatch(
                self.repo,
                679,
                self.head_sha,
                expected_base_sha=self.base_sha,
                dispatch_id="a" * 64,
            )
        self.assertEqual(result["status"], "NOT_PROVEN")
        self.assertEqual(result["run_ids"], [])
        self.assertIn("branch=main", command.call_args.args[0][-1])

    def test_exact_mission_679_orphan_is_not_proven_without_new_evidence(self):
        """The recorded #679 orphan cannot be closed by a time-only scan."""

        writer = GhGitHubWriter(timeout_seconds=5)
        repo = "Igzela/token-efficient-agent-harness-lab"
        mission = "MISSION-RESEARCH-20260901"
        stage = "steward-stage-2-ba5fbc7f811df9f3"
        base = "94f020b7e30b59a96c467977156afb633f826b52"
        head = "99ea6e267a1c2e4a89a1f16da88f98925841eccb"
        intent = f"stage-merge-intent:{mission}:{stage}:679:{head}"
        identity = gh.merge_dispatch_identity(
            repo, 679, base, head, intent_key=intent
        )
        historical_run = {
            "id": 33490744705,
            "status": "completed",
            "conclusion": "success",
            "created_at": "2026-09-01T09:09:34Z",
            "head_sha": "b4f1163295ba6c91b40d01e2cf76777fa296bdbf",
            "head_branch": "main",
            "event": "workflow_dispatch",
            "path": ".github/workflows/agent-merge.yml",
        }
        with patch(
            "subprocess.run",
            return_value=MagicMock(
                returncode=0,
                stdout=json.dumps([{"total_count": 1, "workflow_runs": [historical_run]}]),
                stderr="",
            ),
        ) as command:
            result = writer.reconcile_merge_dispatch(
                repo,
                679,
                head,
                workflow_file="agent-merge.yml",
                not_before="2026-09-01T10:47:53Z",
                expected_base_sha=base,
                dispatch_id=identity["dispatch_id"],
            )
        self.assertEqual(result["status"], "NOT_PROVEN")
        self.assertEqual(result["run_ids"], [])
        request = command.call_args.args[0][-1]
        self.assertIn("branch=main", request)
        self.assertIn("created=>=2026-09-01", request)

    def test_recovery_authorization_cannot_claim_outcome_or_supply_time(self):
        """Owner authority permits recovery but cannot fabricate GitHub facts."""

        writer = GhGitHubWriter(timeout_seconds=5)
        identity = gh.merge_dispatch_identity(
            self.repo,
            679,
            self.base_sha,
            self.head_sha,
            intent_key="merge-intent-orphan",
        )
        marker = {
            "mission_id": "MISSION-ORPHAN",
            "proposal_sha256": "a" * 64,
            "stage_id": "stage-orphan",
            "repository": self.repo,
            "control_issue_number": 208,
            "pr_number": 679,
            "base_sha": self.base_sha,
            "head_sha": self.head_sha,
            "workflow_file": "agent-merge.yml",
            "ref": "main",
            "dispatch_id": identity["dispatch_id"],
            "authorization": "ORPHAN_DISPATCH_RECOVERY",
            "action": "QUARANTINE_EXACT_PR",
            "authorization_id": "owner-recovery-1",
        }
        comment = {
            "id": 999,
            "created_at": "2026-09-01T12:00:01Z",
            "author_association": "OWNER",
            "user": {"login": "Igzela"},
            "body": (
                "owner authorization\n<!-- steward-orphan-dispatch-recovery:v1 "
                + json.dumps(marker, sort_keys=True)
                + " -->"
            ),
        }
        with patch(
            "subprocess.run",
            return_value=MagicMock(
                returncode=0,
                stdout=json.dumps([[comment]]),
                stderr="",
            ),
        ):
            result = writer.read_orphan_dispatch_recovery_authorization(
                self.repo,
                208,
                mission_id="MISSION-ORPHAN",
                proposal_sha256="a" * 64,
                stage_id="stage-orphan",
                pr_number=679,
                expected_base_sha=self.base_sha,
                expected_head_sha=self.head_sha,
                workflow_file="agent-merge.yml",
                dispatch_id=identity["dispatch_id"],
                owner_identity="github:Igzela",
            )
        self.assertIsNotNone(result)
        self.assertEqual(result["comment_id"], 999)
        self.assertEqual(result["owner_identity"], "github:Igzela")
        self.assertEqual(result["comment_created_at"], "2026-09-01T12:00:01Z")
        self.assertNotIn("approved_at", result)

        forbidden = {**marker, "approved_at": "2000-01-01T00:00:00Z"}
        forbidden_comment = {
            **comment,
            "body": "<!-- steward-orphan-dispatch-recovery:v1 "
            + json.dumps(forbidden, sort_keys=True)
            + " -->",
        }
        with patch(
            "subprocess.run",
            return_value=MagicMock(
                returncode=0,
                stdout=json.dumps([[forbidden_comment]]),
                stderr="",
            ),
        ):
            with self.assertRaisesRegex(GitHubFactsError, "marker_invalid"):
                writer.read_orphan_dispatch_recovery_authorization(
                    self.repo,
                    208,
                    mission_id="MISSION-ORPHAN",
                    proposal_sha256="a" * 64,
                    stage_id="stage-orphan",
                    pr_number=679,
                    expected_base_sha=self.base_sha,
                    expected_head_sha=self.head_sha,
                    workflow_file="agent-merge.yml",
                    dispatch_id=identity["dispatch_id"],
                    owner_identity="github:Igzela",
                )

    def test_duplicate_or_replayed_recovery_marker_is_rejected(self):
        """Two exact owner markers cannot create two recovery effects."""

        writer = GhGitHubWriter(timeout_seconds=5)
        identity = gh.merge_dispatch_identity(
            self.repo,
            679,
            self.base_sha,
            self.head_sha,
            intent_key="merge-intent-orphan",
        )
        marker = {
            "mission_id": "MISSION-ORPHAN",
            "proposal_sha256": "a" * 64,
            "stage_id": "stage-orphan",
            "repository": self.repo,
            "control_issue_number": 208,
            "pr_number": 679,
            "base_sha": self.base_sha,
            "head_sha": self.head_sha,
            "workflow_file": "agent-merge.yml",
            "ref": "main",
            "dispatch_id": identity["dispatch_id"],
            "authorization": "ORPHAN_DISPATCH_RECOVERY",
            "action": "QUARANTINE_EXACT_PR",
            "authorization_id": "owner-recovery-replay",
        }

        def comment(comment_id: int) -> dict[str, object]:
            return {
                "id": comment_id,
                "created_at": "2026-09-01T12:00:01Z",
                "author_association": "OWNER",
                "user": {"login": "Igzela"},
                "body": "<!-- steward-orphan-dispatch-recovery:v1 "
                + json.dumps(marker, sort_keys=True)
                + " -->",
            }

        with patch(
            "subprocess.run",
            return_value=MagicMock(
                returncode=0,
                stdout=json.dumps([[comment(1001), comment(1002)]]),
                stderr="",
            ),
        ):
            with self.assertRaisesRegex(GitHubFactsError, "duplicate"):
                writer.read_orphan_dispatch_recovery_authorization(
                    self.repo,
                    208,
                    mission_id="MISSION-ORPHAN",
                    proposal_sha256="a" * 64,
                    stage_id="stage-orphan",
                    pr_number=679,
                    expected_base_sha=self.base_sha,
                    expected_head_sha=self.head_sha,
                    workflow_file="agent-merge.yml",
                    dispatch_id=identity["dispatch_id"],
                    owner_identity="github:Igzela",
                )

    def test_legacy_outcome_marker_is_not_recovery_authority(self):
        """The previously proposed factual marker is ignored by the new reader."""

        writer = GhGitHubWriter(timeout_seconds=5)
        identity = gh.merge_dispatch_identity(
            self.repo,
            679,
            self.base_sha,
            self.head_sha,
            intent_key="merge-intent-orphan",
        )
        legacy = {
            "mission_id": "MISSION-ORPHAN",
            "proposal_sha256": "a" * 64,
            "stage_id": "stage-orphan",
            "repository": self.repo,
            "control_issue_number": 208,
            "pr_number": 679,
            "base_sha": self.base_sha,
            "head_sha": self.head_sha,
            "workflow_file": "agent-merge.yml",
            "ref": "main",
            "dispatch_id": identity["dispatch_id"],
            "resolution": "NO_EFFECT_CONFIRMED",
            "resolution_id": "owner-resolution-legacy",
            "accepted_main_sha": self.base_sha,
            "approved_at": "2026-09-01T12:00:00Z",
        }
        comment = {
            "id": 1000,
            "created_at": "2026-09-01T12:00:01Z",
            "author_association": "OWNER",
            "user": {"login": "Igzela"},
            "body": "<!-- steward-merge-dispatch-resolution:v1 "
            + json.dumps(legacy, sort_keys=True)
            + " -->",
        }
        with patch(
            "subprocess.run",
            return_value=MagicMock(returncode=0, stdout=json.dumps([[comment]]), stderr=""),
        ):
            result = writer.read_orphan_dispatch_recovery_authorization(
                self.repo,
                208,
                mission_id="MISSION-ORPHAN",
                proposal_sha256="a" * 64,
                stage_id="stage-orphan",
                pr_number=679,
                expected_base_sha=self.base_sha,
                expected_head_sha=self.head_sha,
                workflow_file="agent-merge.yml",
                dispatch_id=identity["dispatch_id"],
                owner_identity="github:Igzela",
            )
        self.assertIsNone(result)

    def test_quarantine_requires_exact_base_head_and_readback(self):
        """Quarantine is an exact close-only mutation, never a merge."""

        writer = GhGitHubWriter(timeout_seconds=5)
        open_facts = {
            "repository": self.repo,
            "pr_number": 679,
            "state": "OPEN",
            "merged": False,
            "base_sha": self.base_sha,
            "head_sha": self.head_sha,
        }
        closed_facts = {**open_facts, "state": "CLOSED"}
        with (
            patch.object(writer, "fetch_stage_pr", side_effect=[open_facts, closed_facts]),
            patch.object(writer.reader, "fetch_accepted_main", return_value=self.base_sha),
            patch("subprocess.run", return_value=MagicMock(returncode=0, stdout="", stderr="")) as run,
        ):
            result = writer.quarantine_stage_pr(
                self.repo,
                679,
                expected_base_sha=self.base_sha,
                expected_head_sha=self.head_sha,
            )
        self.assertEqual(result["status"], "CLOSED_UNMERGED")
        self.assertEqual(result["pr_number"], 679)
        self.assertEqual(result["head_sha"], self.head_sha)
        self.assertNotIn("merge", " ".join(run.call_args.args[0]))

    def test_quarantine_read_unavailable_is_not_terminal(self):
        writer = GhGitHubWriter(timeout_seconds=5)
        with patch.object(writer, "fetch_stage_pr", side_effect=GitHubReadError("read_failed")):
            with self.assertRaisesRegex(GitHubReadError, "read_failed"):
                writer.quarantine_stage_pr(
                    self.repo,
                    679,
                    expected_base_sha=self.base_sha,
                    expected_head_sha=self.head_sha,
                )

    def test_quarantine_close_race_reads_merged_winner(self):
        """A close error is reconciled; it cannot hide an old merge winner."""

        writer = GhGitHubWriter(timeout_seconds=5)
        open_facts = {
            "repository": self.repo,
            "pr_number": 679,
            "state": "OPEN",
            "merged": False,
            "base_sha": self.base_sha,
            "head_sha": self.head_sha,
        }
        merged_facts = {**open_facts, "state": "CLOSED", "merged": True}
        with (
            patch.object(writer, "fetch_stage_pr", side_effect=[open_facts, merged_facts]),
            patch.object(
                writer.reader,
                "fetch_accepted_main",
                side_effect=[self.base_sha, self.head_sha],
            ),
            patch(
                "subprocess.run",
                return_value=MagicMock(returncode=1, stdout="", stderr="already merged"),
            ),
        ):
            result = writer.quarantine_stage_pr(
                self.repo,
                679,
                expected_base_sha=self.base_sha,
                expected_head_sha=self.head_sha,
            )
        self.assertEqual(result["status"], "MERGED")
        self.assertEqual(result["accepted_main_sha"], self.head_sha)

    def test_reconcile_merge_dispatch_does_not_match_other_head(self):
        """A workflow for another PR/head cannot resolve this intent."""

        writer = GhGitHubWriter(timeout_seconds=5)
        dispatch_id = "f" * 64
        run_list = json.dumps(
            [
                {"workflow_runs": [{
                    "id": 778,
                    "status": "completed",
                    "conclusion": "failure",
                    "created_at": "2026-08-31T00:00:00Z",
                    "head_sha": self.base_sha,
                    "head_branch": "main",
                    "event": "workflow_dispatch",
                    "path": ".github/workflows/agent-merge.yml@main",
                }]}
            ]
        )
        run_log = (
            "merge PR_NUMBER: 18\nEXPECTED_HEAD: " + ("c" * 40) + "\n"
            f"DISPATCH_ID: {dispatch_id}\n"
        )
        with patch("subprocess.run") as run:
            run.side_effect = [
                MagicMock(returncode=0, stdout=run_list, stderr=""),
                MagicMock(returncode=0, stdout=run_log, stderr=""),
            ]
            result = writer.reconcile_merge_dispatch(
                self.repo,
                17,
                self.head_sha,
                workflow_file="agent-merge.yml",
                dispatch_id=dispatch_id,
            )
        self.assertEqual(result["status"], "NOT_PROVEN")
        self.assertEqual(result["run_ids"], [])

    def test_reconcile_merge_dispatch_does_not_match_pr_number_prefix(self):
        """A similar PR number cannot satisfy exact dispatch binding."""

        writer = GhGitHubWriter(timeout_seconds=5)
        dispatch_id = "1" * 64
        run_list = json.dumps(
            [
                {"workflow_runs": [{
                    "id": 782,
                    "status": "completed",
                    "conclusion": "failure",
                    "created_at": "2026-08-31T00:00:00Z",
                    "head_sha": self.base_sha,
                    "head_branch": "main",
                    "event": "workflow_dispatch",
                    "path": ".github/workflows/agent-merge.yml@main",
                }]}
            ]
        )
        run_log = (
            f"PR_NUMBER: 170\nEXPECTED_HEAD: {self.head_sha}\n"
            f"DISPATCH_ID: {dispatch_id}\n"
        )
        with patch("subprocess.run") as run:
            run.side_effect = [
                MagicMock(returncode=0, stdout=run_list, stderr=""),
                MagicMock(returncode=0, stdout=run_log, stderr=""),
            ]
            result = writer.reconcile_merge_dispatch(
                self.repo,
                17,
                self.head_sha,
                workflow_file="agent-merge.yml",
                dispatch_id=dispatch_id,
            )
        self.assertEqual(result["status"], "NOT_PROVEN")
        self.assertEqual(result["run_ids"], [])

    def test_reconcile_merge_dispatch_holds_when_any_in_window_run_is_active(self):
        """A live canonical run prevents supersession beside an old failure."""

        writer = GhGitHubWriter(timeout_seconds=5)
        run_list = json.dumps(
            [
                {"workflow_runs": [{
                    "id": 779,
                    "status": "in_progress",
                    "conclusion": None,
                    "created_at": "2026-08-31T00:02:00Z",
                    "head_sha": self.base_sha,
                    "head_branch": "main",
                    "event": "workflow_dispatch",
                    "path": ".github/workflows/agent-merge.yml@main",
                },
                {
                    "id": 780,
                    "status": "completed",
                    "conclusion": "failure",
                    "created_at": "2026-08-31T00:01:00Z",
                    "head_sha": self.base_sha,
                    "head_branch": "main",
                    "event": "workflow_dispatch",
                    "path": ".github/workflows/agent-merge.yml@main",
                }]}]
        )
        dispatch_id = "2" * 64
        failed_log = (
            f"PR_NUMBER: 17\nEXPECTED_HEAD: {self.head_sha}\n"
            f"DISPATCH_ID: {dispatch_id}\n"
        )
        with patch("subprocess.run") as run:
            run.side_effect = [
                MagicMock(returncode=0, stdout=run_list, stderr=""),
                MagicMock(returncode=0, stdout=failed_log, stderr=""),
            ]
            result = writer.reconcile_merge_dispatch(
                self.repo,
                17,
                self.head_sha,
                workflow_file="agent-merge.yml",
                not_before="2026-08-31T00:00:00Z",
                dispatch_id=dispatch_id,
            )
        self.assertEqual(result["status"], "PENDING")
        self.assertEqual(result["run_ids"], [779, 780])

    def test_reconcile_merge_dispatch_rejects_unknown_run_status(self):
        """Malformed provider status cannot prove a no-effect dispatch."""

        writer = GhGitHubWriter(timeout_seconds=5)
        run_list = json.dumps(
            [
                {"workflow_runs": [{
                    "id": 781,
                    "status": "mystery",
                    "conclusion": "failure",
                    "created_at": "2026-08-31T00:00:00Z",
                    "head_sha": self.base_sha,
                    "head_branch": "main",
                    "event": "workflow_dispatch",
                    "path": ".github/workflows/agent-merge.yml@main",
                }]}
            ]
        )
        with patch(
            "subprocess.run",
            return_value=MagicMock(returncode=0, stdout=run_list, stderr=""),
        ):
            with self.assertRaisesRegex(GitHubReadError, "status_malformed"):
                writer.reconcile_merge_dispatch(
                    self.repo, 17, self.head_sha, dispatch_id="3" * 64
                )

    def test_post_merge_readback_authoritative_sha(self):
        """Verify post_merge_readback queries GitHub main branch SHA and validates integrity."""
        writer = GhGitHubWriter(timeout_seconds=5)

        merged_pr = {
            "number": 304,
            "state": "closed",
            "merged": True,
            "head": {"sha": self.head_sha},
            "merge_commit_sha": "d" * 40,
        }
        branch = {"commit": {"sha": "d" * 40}}
        with patch("subprocess.run") as mock_run:
            mock_run.side_effect = [
                MagicMock(returncode=0, stdout=json.dumps(merged_pr), stderr=""),
                MagicMock(returncode=0, stdout=json.dumps(branch), stderr=""),
            ]
            readback = writer.post_merge_readback(self.repo, 304, self.head_sha)
            self.assertEqual(readback["schema_version"], "post_merge_readback.v2")
            self.assertEqual(readback["accepted_main_sha"], "d" * 40)
            self.assertEqual(readback["status"], "VERIFIED")

        # Reject a main tip that cannot be proved to be the PR merge commit.
        with patch("subprocess.run") as mock_run:
            mock_run.side_effect = [
                MagicMock(returncode=0, stdout=json.dumps(merged_pr), stderr=""),
                MagicMock(returncode=0, stdout=json.dumps({"commit": {"sha": "e" * 40}}), stderr=""),
            ]
            with self.assertRaises(GitHubFactsError):
                writer.post_merge_readback(self.repo, 304, self.head_sha)


if __name__ == "__main__":
    unittest.main()
