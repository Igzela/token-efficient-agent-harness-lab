import importlib.util
import json
from pathlib import Path
import sys
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "project_context.py"
SPEC = importlib.util.spec_from_file_location("project_context", SCRIPT)
assert SPEC and SPEC.loader
project_context = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = project_context
SPEC.loader.exec_module(project_context)


class TestProjectContextRouting(unittest.TestCase):
    def test_registered_campaign_contract_provides_fallback_route(self):
        source = (
            Path(__file__).resolve().parents[1]
            / "scripts"
            / "agent-control"
            / "mission_contract.py"
        ).read_text(encoding="utf-8")

        self.assertEqual(
            project_context.parse_registered_campaign_mission(source),
            {
                "mission_id": "AUTONOMOUS-STEWARD-MIGRATION-2026-08-27",
                "state": "IDLE",
                "pr_number": None,
            },
        )

    def test_ready_live_mission_does_not_infer_pr_from_prerequisites(self):
        text = """\
## Active Routing

1. `PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1` — `READY_FOR_EXECUTION`: satisfied by PRs #339/#340 and #342.

## Mission PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** Missions A and B are accepted by PR #342.
"""

        parsed = project_context.parse_first_routed_mission(text)

        self.assertEqual(
            parsed,
            {
                "mission_id": "PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1",
                "state": "READY_FOR_EXECUTION",
                "pr_number": None,
            },
        )

    def test_explicit_owned_pr_remains_active_pr_binding(self):
        text = """\
## Active Routing

1. `PE7-TEST-1` — `IN_PROGRESS`

## Mission PE7-TEST-1

**State:** `IN_PROGRESS`

**Owned PR:** #342
"""

        parsed = project_context.parse_first_routed_mission(text)

        self.assertEqual(parsed["mission_id"], "PE7-TEST-1")
        self.assertEqual(parsed["state"], "IN_PROGRESS")
        self.assertEqual(parsed["pr_number"], "342")

    def test_ready_mission_without_pr_does_not_infer_implementation_pr(self):
        action = project_context.next_permitted_action(
            {
                "mission_id": "PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1",
                "state": "READY_FOR_EXECUTION",
            },
            None,
        )

        self.assertIn("documented prerequisites", action)
        self.assertNotIn("create or continue one focused PR", action)

    def test_in_progress_prerequisite_prose_does_not_use_legacy_fallback(self):
        text = """\
## Active Routing

1. `PE7-TEST-1` — `IN_PROGRESS`

## Mission PE7-TEST-1

**State:** `IN_PROGRESS`

Prerequisite: PR #342 is accepted.
"""

        self.assertIsNone(project_context.parse_first_routed_mission(text)["pr_number"])

    def test_owner_direct_repair_binding_requires_current_owner_bound_draft_pr(self):
        repository = "Igzela/token-efficient-agent-harness-lab"
        head = "a" * 40
        observer = _OwnerDirectObserver(
            repository,
            pull={
                "number": 710,
                "state": "open",
                "draft": True,
                "base": {"ref": "main", "sha": "b" * 40},
                "head": {"ref": "codex/repair", "sha": head},
            },
            comments=[_owner_direct_comment(repository, 710, head)],
        )

        binding = project_context.read_owner_direct_repair_binding(
            repository, 710, observer=observer
        )

        self.assertEqual(binding["repository"], repository)
        self.assertEqual(binding["pr_number"], 710)
        self.assertEqual(binding["head_sha"], head)
        self.assertEqual(binding["owner_identity"], "github:Igzela")
        self.assertEqual(binding["dispatch_lane"], "owner_direct_existing_pr_repair")
        self.assertEqual(binding["allowed_paths"], ["scripts/", "tests/"])

    def test_owner_direct_repair_preserves_stale_markers_for_later_rounds(self):
        repository = "Igzela/token-efficient-agent-harness-lab"
        current_head = "a" * 40
        observer = _OwnerDirectObserver(
            repository,
            pull={
                "number": 710,
                "state": "open",
                "draft": True,
                "base": {"ref": "main", "sha": "b" * 40},
                "head": {"ref": "codex/repair", "sha": current_head},
            },
            comments=[
                _owner_direct_comment(repository, 710, "c" * 40, comment_id=9000),
                _owner_direct_comment(repository, 710, current_head, comment_id=9001),
            ],
        )

        binding = project_context.read_owner_direct_repair_binding(
            repository, 710, observer=observer
        )

        self.assertEqual(binding["head_sha"], current_head)
        self.assertEqual(binding["owner_comment_id"], 9001)

    def test_owner_direct_repair_rejects_multiple_current_markers(self):
        repository = "Igzela/token-efficient-agent-harness-lab"
        current_head = "a" * 40
        observer = _OwnerDirectObserver(
            repository,
            pull={
                "number": 710,
                "state": "open",
                "draft": True,
                "base": {"ref": "main", "sha": "b" * 40},
                "head": {"ref": "codex/repair", "sha": current_head},
            },
            comments=[
                _owner_direct_comment(repository, 710, current_head, comment_id=9001),
                _owner_direct_comment(repository, 710, current_head, comment_id=9002),
            ],
        )

        with self.assertRaisesRegex(
            project_context.GitHubObservationError,
            "owner_direct_repair_binding_ambiguous",
        ):
            project_context.read_owner_direct_repair_binding(
                repository, 710, observer=observer
            )

    def test_owner_direct_repair_binding_rejects_stale_identity_and_missing_owner(self):
        repository = "Igzela/token-efficient-agent-harness-lab"
        current_head = "a" * 40
        cases = (
            (
                "owner_direct_repair_head_stale",
                _OwnerDirectObserver(
                    repository,
                    pull={
                        "number": 710,
                        "state": "open",
                        "draft": True,
                        "base": {"ref": "main", "sha": "b" * 40},
                        "head": {"ref": "codex/repair", "sha": current_head},
                    },
                    comments=[_owner_direct_comment(repository, 710, "c" * 40)],
                ),
            ),
            (
                "owner_direct_repair_repository_mismatch",
                _OwnerDirectObserver(
                    repository,
                    pull={
                        "number": 710,
                        "state": "open",
                        "draft": True,
                        "base": {"ref": "main", "sha": "b" * 40},
                        "head": {"ref": "codex/repair", "sha": current_head},
                    },
                    comments=[
                        _owner_direct_comment(
                            repository,
                            710,
                            current_head,
                            marker_repository="other/repository",
                        )
                    ],
                ),
            ),
            (
                "owner_direct_repair_owner_required",
                _OwnerDirectObserver(
                    repository,
                    pull={
                        "number": 710,
                        "state": "open",
                        "draft": True,
                        "base": {"ref": "main", "sha": "b" * 40},
                        "head": {"ref": "codex/repair", "sha": current_head},
                    },
                    comments=[
                        _owner_direct_comment(
                            repository, 710, current_head, association="MEMBER"
                        )
                    ],
                ),
            ),
            (
                "owner_direct_repair_binding_missing",
                _OwnerDirectObserver(
                    repository,
                    pull={
                        "number": 710,
                        "state": "open",
                        "draft": True,
                        "base": {"ref": "main", "sha": "b" * 40},
                        "head": {"ref": "codex/repair", "sha": current_head},
                    },
                    comments=[],
                ),
            ),
            (
                "owner_direct_repair_pr_mismatch",
                _OwnerDirectObserver(
                    repository,
                    pull={
                        "number": 710,
                        "state": "open",
                        "draft": True,
                        "base": {"ref": "main", "sha": "b" * 40},
                        "head": {"ref": "codex/repair", "sha": current_head},
                    },
                    comments=[_owner_direct_comment(repository, 711, current_head)],
                ),
            ),
        )
        for reason, observer in cases:
            with self.subTest(reason=reason):
                with self.assertRaisesRegex(project_context.GitHubObservationError, reason):
                    project_context.read_owner_direct_repair_binding(
                        repository, 710, observer=observer
                    )

    def test_owner_direct_capsule_separates_continuity_from_execution_authority(self):
        repository = "Igzela/token-efficient-agent-harness-lab"
        head = "a" * 40
        observer = _OwnerDirectObserver(
            repository,
            pull={
                "number": 710,
                "state": "open",
                "draft": True,
                "base": {"ref": "main", "sha": "b" * 40},
                "head": {"ref": "codex/repair", "sha": head},
            },
            comments=[_owner_direct_comment(repository, 710, head)],
            frontiers=[],
        )
        documents = {
            "START_HERE.md": "# Start Here\n",
            "AGENTS.md": "# Agents\n",
            "README.md": "# README\n",
            "docs/ARCHITECTURE.md": "# Architecture\n",
            "docs/AUTONOMY.md": "# Autonomy\n",
            "docs/ROADMAP.md": "# Roadmap\n",
            "docs/RUNBOOK.md": "# Runbook\n",
        }
        with (
            mock.patch.object(
                project_context, "accepted_baseline", return_value={
                    "availability": "confirmed", "branch": "main", "sha": "b" * 40,
                    "source": "fixture",
                }
            ),
            mock.patch.object(
                project_context, "canonical_documents", return_value={
                    "availability": "confirmed", "source_sha": "b" * 40,
                    "documents": documents,
                }
            ),
            mock.patch.object(
                project_context, "local_checkout_state", return_value={
                    "head_sha": head, "branch": "codex/repair", "detached": False,
                    "dirty": False, "change_count": 0,
                }
            ),
        ):
            capsule = project_context.build_capsule(
                offline=False,
                repository=repository,
                owner_direct_repair_pr_number=710,
                observer=observer,
            )
        self.assertEqual(
            capsule["steward_continuity"]["reason"],
            "steward_continuity_unavailable",
        )
        self.assertEqual(capsule["execution_authority"]["availability"], "confirmed")
        self.assertEqual(
            capsule["binding"]["owner_direct_repair"]["head_sha"], head
        )


class TestReviewStateProjection(unittest.TestCase):
    def test_deferred_non_blocking_review_does_not_revoke_exact_head_receipt(self):
        head = "a" * 40
        base = "b" * 40
        receipt = f"""EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {base}...{head}
Reviewer session identity: independent-session-1
Reviewer authenticated identity: reviewer
Review transport: direct-github-reviewer
Observed at: 2026-08-01T06:00:00Z
Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: PASS
Unresolved objections: none
"""

        for note in ("Deferred/non-blocking note", "Deferred note"):
            with self.subTest(note=note):
                observation = project_context._build_review_observation(
                    head_sha=head,
                    base_sha=base,
                    pr_author_identity="implementation-agent",
                    aggregate_review="REVIEW_REQUIRED",
                    reviews=[{"state": "COMMENTED", "body": note}],
                    comments=[{"author": {"login": "reviewer"}, "body": receipt}],
                    observation_time="2026-08-01T06:00:00Z",
                )
                self.assertEqual(
                    observation["exact_head_review_state"], "confirmed"
                )
                self.assertEqual(
                    observation["unresolved_objections_state"], "none_observed"
                )

    def test_offline_projection_is_unavailable_with_bounded_keys(self):
        pr = project_context.load_pr(
            "Igzela/token-efficient-agent-harness-lab", 364, offline=True
        )
        projection = pr["review_state_projection"]
        self.assertEqual(projection["availability"], "unavailable")
        for key in (
            "review_protocol_version",
            "review_mode",
            "review_round",
            "prior_reviewed_head",
            "reviewed_head",
            "finding_ledger_digest",
            "open_blocker_ids",
            "deferred_note_ids",
            "autonomous_repairs_remaining",
            "stop_reason",
            "review_state",
        ):
            self.assertIn(key, projection)

    def test_linked_issue_not_found_marks_unavailable(self):
        projection = project_context._load_review_state_projection(
            "Igzela/token-efficient-agent-harness-lab",
            {"headRefOid": "a" * 40, "body": "no linked issue marker"},
        )
        self.assertEqual(projection["availability"], "unavailable")
        self.assertEqual(projection["unavailable_reason"], "linked_issue_not_found")

    def test_missing_head_marks_unavailable(self):
        projection = project_context._load_review_state_projection(
            "Igzela/token-efficient-agent-harness-lab",
            {"body": "Closes #1"},
        )
        self.assertEqual(projection["availability"], "unavailable")

    def test_capsule_binding_includes_review_state_projection(self):
        payload = project_context.build_capsule(
            offline=True,
            repository="Igzela/token-efficient-agent-harness-lab",
        )
        projection = payload["binding"]["review_state_projection"]
        self.assertIsInstance(projection, dict)
        self.assertIn("availability", projection)
        self.assertIn("review_state", projection)
        for forbidden in ("findings", "severity", "acceptance_condition", "disposition"):
            self.assertNotIn(forbidden, projection)
        self.assertEqual(
            payload["steward_continuity"]["reason"],
            "steward_continuity_unavailable",
        )
        self.assertEqual(
            payload["execution_authority"]["reason"],
            "execution_authority_unavailable",
        )

    def test_project_capsule_fields_conflict_and_legacy_paths(self):
        import sys as _sys

        _sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts" / "agent-control"))
        import review_convergence as rc

        v3_state = {
            "kind": "agent-orchestrator-review-state",
            "version": 3,
            "issue_number": 42,
            "pr_number": 207,
            "head_sha": "a" * 40,
            "verdict": "PASS",
            "summary": "ok",
            "base_sha": "c" * 40,
            "reviewed_range": f"{'c' * 40}...{'a' * 40}",
            "review_mode": "full",
            "review_round": 1,
            "prior_reviewed_head": "",
            "findings": [],
            "finding_ledger_digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945",
            "open_blocker_ids": [],
            "deferred_note_ids": [],
            "decision_required_ids": [],
            "autonomous_repairs_remaining": 1,
            "stop_reason": "",
            "artifact_sha256": "",
            "review_workflow_run_id": None,
            "blockers": [],
            "major_notes": [],
            "minor_notes": [],
            "review_protocol_version": rc.REVIEW_PROTOCOL_VERSION,
        }
        confirmed = rc.project_capsule_fields(v3_state, expected_head="a" * 40)
        self.assertEqual(confirmed["availability"], "confirmed")
        conflict = rc.project_capsule_fields(v3_state, expected_head="b" * 40)
        self.assertEqual(conflict["availability"], "conflict")
        legacy = rc.project_capsule_fields({**v3_state, "version": 2}, expected_head="a" * 40)
        self.assertEqual(legacy["availability"], "legacy")
        self.assertIsNone(legacy["review_round"])


class _OwnerDirectObserver:
    def __init__(self, repository, *, pull, comments, frontiers=None):
        self.repository = repository
        self.pull = pull
        self.comments = comments
        self.frontiers = frontiers or []

    def list_open_pull_requests(self, *, base="main"):
        return self.frontiers

    def pull_request(self, number):
        return self.pull

    def issue_comments(self, number):
        return self.comments


def _owner_direct_comment(
    repository,
    pr_number,
    head_sha,
    *,
    association="OWNER",
    marker_repository=None,
    comment_id=9001,
):
    marker = {
        "action": "OWNER_DIRECT_EXISTING_PR_REPAIR",
        "authorization_id": "repair-1",
        "repository": marker_repository or repository,
        "pr_number": pr_number,
        "head_sha": head_sha,
        "head_branch": "codex/repair",
        "allowed_paths": ["scripts/", "tests/"],
        "verification": [
            "git diff --check",
            "uv run --no-project python -m unittest tests.test_session_context",
        ],
    }
    return {
        "id": comment_id,
        "issue_url": (
            f"https://api.github.com/repos/{repository}/issues/{pr_number}"
        ),
        "author_association": association,
        "user": {"login": "Igzela"},
        "body": (
            "owner repair authorization\n"
            f"<!-- steward-owner-direct-repair:v1 {json.dumps(marker, sort_keys=True)} -->"
        ),
    }


if __name__ == "__main__":
    unittest.main()
