import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.advisor import AdvisorResponse
from harness_core.skills import SkillExtractor, SkillRecord, SkillStore
from harness_core.task_records import TaskRecordBundle

FIXTURE_TASK_DIR = (
    Path(__file__).resolve().parents[1]
    / "docs"
    / "stage0"
    / "tasks"
    / "task-005-failure-fix-loop"
)


def make_bundle_with_run_log():
    return TaskRecordBundle(
        task_dir=FIXTURE_TASK_DIR,
        task_spec={"task_id": "task_001", "type": "bugfix"},
        completion={"status": "completed", "failure_code": "F007_TEST_FAILURE"},
        handoff_pack={"summary": "done"},
        events_path=FIXTURE_TASK_DIR / "events.jsonl",
        run_log_path=FIXTURE_TASK_DIR / "run_log.md",
        run_log_text="Fixed by updating the test fixture.\nRoot cause: missing null check.\nApproach: validate input before processing.",
    )


def make_bundle_empty_log():
    return TaskRecordBundle(
        task_dir=FIXTURE_TASK_DIR,
        task_spec={"task_id": "task_002", "type": "code_small_change"},
        completion={"status": "completed"},
        handoff_pack={"summary": "done"},
        events_path=FIXTURE_TASK_DIR / "events.jsonl",
        run_log_text="",
    )


def make_advisor_response():
    return AdvisorResponse(
        call_type="correction",
        diagnosis="Test failure detected",
        recommended_action="Fix the failing test first",
        do_not_do="Do not skip test validation",
        confidence=0.85,
        token_usage=100,
        provider="stub",
    )


class SkillExtractorTests(unittest.TestCase):
    def test_extract_from_run_log_patterns(self):
        extractor = SkillExtractor()
        bundle = make_bundle_with_run_log()
        skills = extractor.extract_from_bundle(bundle)
        self.assertGreater(len(skills), 0)
        types = {s.skill_type for s in skills}
        self.assertIn("fix_pattern", types)

    def test_extract_from_completion_failure_code(self):
        extractor = SkillExtractor()
        bundle = make_bundle_with_run_log()
        skills = extractor.extract_from_bundle(bundle)
        failure_skills = [s for s in skills if "F007" in s.title]
        self.assertGreater(len(failure_skills), 0)

    def test_empty_log_returns_no_skills(self):
        extractor = SkillExtractor()
        bundle = make_bundle_empty_log()
        skills = extractor.extract_from_bundle(bundle)
        self.assertEqual(0, len(skills))

    def test_extract_from_advisor_response(self):
        extractor = SkillExtractor()
        resp = make_advisor_response()
        skills = extractor.extract_from_advisor(resp, "task_001")
        self.assertGreater(len(skills), 0)
        self.assertTrue(any(s.extracted_from == "advisor" for s in skills))

    def test_advisor_recommended_action_extracted(self):
        extractor = SkillExtractor()
        resp = make_advisor_response()
        skills = extractor.extract_from_advisor(resp, "task_001")
        approach_skills = [s for s in skills if s.skill_type == "approach"]
        self.assertGreater(len(approach_skills), 0)

    def test_advisor_do_not_do_extracted(self):
        extractor = SkillExtractor()
        resp = make_advisor_response()
        skills = extractor.extract_from_advisor(resp, "task_001")
        warning_skills = [s for s in skills if "warning" in s.title.lower()]
        self.assertGreater(len(warning_skills), 0)

    def test_no_prompt_mutation(self):
        extractor = SkillExtractor()
        bundle = make_bundle_with_run_log()
        original_text = bundle.run_log_text
        extractor.extract_from_bundle(bundle)
        self.assertEqual(original_text, bundle.run_log_text)

    def test_deterministic_skill_ids(self):
        extractor = SkillExtractor()
        resp = make_advisor_response()
        s1 = extractor.extract_from_advisor(resp, "task_001")
        s2 = extractor.extract_from_advisor(resp, "task_001")
        self.assertEqual(s1[0].skill_id, s2[0].skill_id)


class SkillStoreTests(unittest.TestCase):
    def test_save_and_load(self):
        with tempfile.TemporaryDirectory() as tmp:
            store = SkillStore(Path(tmp))
            skill = SkillRecord(
                skill_id="skill_test001",
                source_task_id="task_001",
                skill_type="fix_pattern",
                title="Test skill",
                description="A test skill",
                applicable_when="during testing",
                evidence_refs=("run_log",),
                confidence=0.8,
                extracted_from="run_log",
            )
            store.save(skill)
            loaded = store.load("skill_test001")
            self.assertIsNotNone(loaded)
            self.assertEqual("skill_test001", loaded.skill_id)
            self.assertEqual("fix_pattern", loaded.skill_type)

    def test_load_nonexistent_returns_none(self):
        with tempfile.TemporaryDirectory() as tmp:
            store = SkillStore(Path(tmp))
            self.assertIsNone(store.load("nonexistent"))

    def test_list_skills(self):
        with tempfile.TemporaryDirectory() as tmp:
            store = SkillStore(Path(tmp))
            for i in range(3):
                store.save(
                    SkillRecord(
                        skill_id=f"skill_{i:03d}",
                        source_task_id="task_001",
                        skill_type="approach",
                        title=f"Skill {i}",
                        description=f"Description {i}",
                        applicable_when="always",
                        evidence_refs=(),
                        confidence=0.7,
                        extracted_from="run_log",
                    )
                )
            skills = store.list_skills()
            self.assertEqual(3, len(skills))

    def test_search_by_title(self):
        with tempfile.TemporaryDirectory() as tmp:
            store = SkillStore(Path(tmp))
            store.save(
                SkillRecord(
                    skill_id="skill_db",
                    source_task_id="task_001",
                    skill_type="fix_pattern",
                    title="Database connection fix",
                    description="Fix DB connection timeout",
                    applicable_when="DB timeout",
                    evidence_refs=(),
                    confidence=0.8,
                    extracted_from="run_log",
                )
            )
            store.save(
                SkillRecord(
                    skill_id="skill_ui",
                    source_task_id="task_002",
                    skill_type="approach",
                    title="UI rendering approach",
                    description="Use lazy loading for UI",
                    applicable_when="UI perf",
                    evidence_refs=(),
                    confidence=0.7,
                    extracted_from="retrospective",
                )
            )
            results = store.search("database")
            self.assertEqual(1, len(results))
            self.assertEqual("skill_db", results[0].skill_id)

    def test_search_by_description(self):
        with tempfile.TemporaryDirectory() as tmp:
            store = SkillStore(Path(tmp))
            store.save(
                SkillRecord(
                    skill_id="skill_001",
                    source_task_id="task_001",
                    skill_type="approach",
                    title="General skill",
                    description="Handle timeout errors gracefully",
                    applicable_when="timeout",
                    evidence_refs=(),
                    confidence=0.7,
                    extracted_from="run_log",
                )
            )
            results = store.search("timeout")
            self.assertEqual(1, len(results))

    def test_empty_store_list(self):
        with tempfile.TemporaryDirectory() as tmp:
            store = SkillStore(Path(tmp))
            self.assertEqual(0, len(store.list_skills()))

    def test_empty_store_search(self):
        with tempfile.TemporaryDirectory() as tmp:
            store = SkillStore(Path(tmp))
            self.assertEqual(0, len(store.search("anything")))


if __name__ == "__main__":
    unittest.main()
