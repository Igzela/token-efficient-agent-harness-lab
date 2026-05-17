import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import (
    ArtifactGate,
    FinalGateRunner,
    QualityDigestGenerator,
    QualityGateManager,
    ScoringEngine,
    TaskRecordStore,
    TrajectoryReport,
    generate_batch_digest,
    replay_all,
    validate_allowed_files_completeness,
    validate_advisor_protocol_events,
    validate_failure_code,
    validate_replay_preflight_check,
)


FIXTURE_BASE = Path(__file__).resolve().parent / "fixtures" / "real_world_eval"


def project_path(name: str) -> Path:
    return FIXTURE_BASE / name


def event_log_path(fixture: Path) -> Path:
    project_events = fixture / "project_events.jsonl"
    if project_events.exists():
        return project_events
    return fixture / "events.jsonl"


def task_root(fixture: Path) -> Path:
    records = fixture / "task-records"
    if records.exists():
        return records
    return fixture / "tasks"


def task_dir(fixture: Path, task_id: str) -> Path:
    return task_root(fixture) / task_id


def snapshot_fixture(fixture: Path) -> dict[Path, bytes]:
    return {
        path: path.read_bytes()
        for path in sorted(fixture.rglob("*"))
        if path.is_file()
    }


def load_bundle(fixture: Path, task_id: str):
    store = TaskRecordStore(task_root(fixture))
    report = store.validate_task_bundle(task_dir(fixture, task_id))
    assert report.ok, report.errors
    assert report.bundle is not None
    return report.bundle


def evaluate_task(bundle):
    final_gate = FinalGateRunner().evaluate(bundle, current_item_status="review")
    artifact_gate = ArtifactGate().evaluate(
        bundle,
        allowed_files=tuple(bundle.task_spec.get("allowed_files", ())),
        forbidden_files=tuple(bundle.task_spec.get("forbidden_files", ())),
    )
    score = ScoringEngine().score_task_bundle(bundle, final_gate)
    quality_gate = QualityGateManager().evaluate(
        bundle,
        final_gate,
        artifact_gate,
        trajectory_report=TrajectoryReport(),
        task_score=score,
    )
    return final_gate, artifact_gate, score, quality_gate


class RealWorldReadOnlyEvaluationTests(unittest.TestCase):
    def assertFixtureUnchanged(self, fixture: Path, before: dict[Path, bytes]) -> None:
        self.assertEqual(before, snapshot_fixture(fixture))

    def assertProjectReplayPasses(self, fixture: Path, completed_items: tuple[str, ...]):
        validation = validate_replay_preflight_check(event_log_path(fixture))
        self.assertTrue(validation.ok, validation.errors)

        projections = replay_all(event_log_path(fixture))
        digest = generate_batch_digest(projections)
        self.assertEqual(completed_items, digest.completed_items)
        return projections, digest

    def test_existing_first_pass_fixture_still_passes(self):
        fixture = project_path("project-alpha")
        before = snapshot_fixture(fixture)

        projections, digest = self.assertProjectReplayPasses(
            fixture,
            ("issue_001_doc_cleanup",),
        )
        self.assertEqual(1, digest.handoff_count)
        self.assertEqual(0, digest.resolved_dependency_count)
        self.assertEqual("done", projections.project.items["issue_001_doc_cleanup"].status)

        bundle = load_bundle(fixture, "issue-001-doc-cleanup")
        final_gate, _, score, _ = evaluate_task(bundle)
        self.assertEqual("pass", final_gate.result)
        self.assertEqual("issue_001_doc_cleanup", score.task_id)
        self.assertGreaterEqual(score.weighted_score, 0.60)

        self.assertFixtureUnchanged(fixture, before)

    def test_doc_update_fixture_loads_scores_and_digests(self):
        fixture = project_path("doc-update-project")
        before = snapshot_fixture(fixture)

        _, digest = self.assertProjectReplayPasses(fixture, ("doc_update_001",))
        self.assertEqual(1, digest.handoff_count)

        bundle = load_bundle(fixture, "doc_update_001")
        final_gate, artifact_gate, score, quality_gate = evaluate_task(bundle)
        self.assertEqual("pass", final_gate.result)
        self.assertTrue(artifact_gate.ok)
        self.assertGreaterEqual(score.weighted_score, 0.75)
        self.assertEqual("pass", quality_gate.result)

        self.assertFixtureUnchanged(fixture, before)

    def test_bugfix_fixture_exercises_artifact_gate_and_scoring(self):
        fixture = project_path("bugfix-project")
        before = snapshot_fixture(fixture)

        self.assertProjectReplayPasses(fixture, ("bugfix_001",))
        bundle = load_bundle(fixture, "bugfix_001")
        final_gate, artifact_gate, score, quality_gate = evaluate_task(bundle)

        self.assertEqual("pass", final_gate.result)
        self.assertTrue(artifact_gate.ok, artifact_gate)
        self.assertEqual((), artifact_gate.missing_artifacts)
        self.assertGreaterEqual(score.weighted_score, 0.75)
        self.assertEqual("pass", quality_gate.result)

        self.assertFixtureUnchanged(fixture, before)

    def test_config_rule_fixture_preserves_file_policy_path(self):
        fixture = project_path("config-rule-project")
        before = snapshot_fixture(fixture)

        self.assertProjectReplayPasses(fixture, ("config_rule_001",))
        bundle = load_bundle(fixture, "config_rule_001")
        _, artifact_gate, score, _ = evaluate_task(bundle)

        required_files = tuple(bundle.task_spec["required_files"])
        allowed_files = tuple(bundle.task_spec["allowed_files"])
        allowed_result = validate_allowed_files_completeness(allowed_files, required_files)
        self.assertTrue(allowed_result.ok, allowed_result.errors)
        self.assertTrue(artifact_gate.ok, artifact_gate)
        self.assertEqual((), artifact_gate.forbidden_violations)
        self.assertGreaterEqual(score.weighted_score, 0.75)

        self.assertFixtureUnchanged(fixture, before)

    def test_failure_fix_loop_fixture_accepts_failure_code_and_scores_penalty(self):
        fixture = project_path("failure-fix-loop-project")
        before = snapshot_fixture(fixture)

        self.assertProjectReplayPasses(fixture, ("failure_fix_loop_001",))
        bundle = load_bundle(fixture, "failure_fix_loop_001")

        failure_result = validate_failure_code(
            bundle.task_spec["failure_code"],
            bundle.task_spec["failure_subcode"],
        )
        self.assertTrue(failure_result.ok, failure_result.errors)

        advisor_result = validate_advisor_protocol_events(
            bundle.events_path,
            expected_min_advisor_calls=2,
        )
        self.assertTrue(advisor_result.ok, advisor_result.errors)

        final_gate, artifact_gate, score, quality_gate = evaluate_task(bundle)
        self.assertEqual("pass", final_gate.result)
        self.assertTrue(artifact_gate.ok, artifact_gate)
        self.assertEqual(-0.20, score.failure_code_penalty)
        self.assertTrue(any("failure_code" in penalty for penalty in score.penalties))
        self.assertIn(quality_gate.result, ("pass", "pass_with_notes"))

        self.assertFixtureUnchanged(fixture, before)

    def test_cross_task_dependency_fixture_projects_and_quality_digests(self):
        fixture = project_path("cross-task-dependency-project")
        before = snapshot_fixture(fixture)

        projections, digest = self.assertProjectReplayPasses(
            fixture,
            ("consumer_docs_001", "schema_contract_001"),
        )
        self.assertEqual(2, len(projections.project.items))
        self.assertEqual(2, digest.handoff_count)
        self.assertEqual(1, digest.resolved_dependency_count)
        self.assertEqual("schema_contract_001", projections.dependencies.resolved[0].from_node)
        self.assertEqual("consumer_docs_001", projections.dependencies.resolved[0].to_node)

        task_scores = []
        gate_decisions = {}
        for item_id in ("schema_contract_001", "consumer_docs_001"):
            bundle = load_bundle(fixture, item_id)
            final_gate, artifact_gate, score, quality_gate = evaluate_task(bundle)
            self.assertEqual("pass", final_gate.result)
            self.assertTrue(artifact_gate.ok, artifact_gate)
            task_scores.append(score)
            gate_decisions[item_id] = quality_gate

        run_score = ScoringEngine().score_run(tuple(task_scores))
        quality_digest = QualityDigestGenerator().generate(
            projections=projections,
            run_score=run_score,
            gate_decisions=gate_decisions,
            trajectory=TrajectoryReport(),
        )

        self.assertEqual(2, len(quality_digest.items))
        self.assertGreaterEqual(quality_digest.aggregate_score, 0.75)
        self.assertTrue(quality_digest.trajectory_ok)
        self.assertEqual((), quality_digest.recommended_actions)

        self.assertFixtureUnchanged(fixture, before)


if __name__ == "__main__":
    unittest.main()
