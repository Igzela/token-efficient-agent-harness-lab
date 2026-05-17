import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import (
    FinalGateRunner,
    ScoringEngine,
    TaskRecordStore,
    generate_batch_digest,
    replay_all,
    validate_replay_preflight_check,
)


FIXTURE_ROOT = (
    Path(__file__).resolve().parent / "fixtures" / "real_world_eval" / "project-alpha"
)
PROJECT_EVENTS = FIXTURE_ROOT / "project_events.jsonl"
TASK_RECORDS = FIXTURE_ROOT / "task-records"
TASK_DIR = TASK_RECORDS / "issue-001-doc-cleanup"
READ_ONLY_PATHS = (
    PROJECT_EVENTS,
    TASK_DIR / "task_spec.json",
    TASK_DIR / "events.jsonl",
    TASK_DIR / "completion.json",
    TASK_DIR / "handoff_pack.json",
    TASK_DIR / "run_log.md",
)


class RealWorldReadOnlyEvaluationTests(unittest.TestCase):
    def test_copied_fixture_runs_existing_read_only_evaluation_apis(self):
        before = {path: path.read_bytes() for path in READ_ONLY_PATHS}

        validation = validate_replay_preflight_check(PROJECT_EVENTS)
        self.assertTrue(validation.ok, validation.errors)

        projections = replay_all(PROJECT_EVENTS)
        digest = generate_batch_digest(projections)
        self.assertEqual(("issue_001_doc_cleanup",), digest.completed_items)
        self.assertEqual(1, digest.handoff_count)
        self.assertEqual(0, digest.resolved_dependency_count)

        store = TaskRecordStore(TASK_RECORDS)
        report = store.validate_task_bundle(TASK_DIR)
        self.assertTrue(report.ok, report.errors)
        self.assertIsNotNone(report.bundle)

        bundle = report.bundle
        decision = FinalGateRunner().evaluate(bundle, current_item_status="review")
        self.assertEqual("pass", decision.result)
        self.assertEqual("done", decision.next_project_status)

        score = ScoringEngine().score_task_bundle(bundle, decision)
        self.assertEqual("issue_001_doc_cleanup", score.task_id)
        self.assertGreaterEqual(score.weighted_score, 0.60)

        after = {path: path.read_bytes() for path in READ_ONLY_PATHS}
        self.assertEqual(before, after)


if __name__ == "__main__":
    unittest.main()
