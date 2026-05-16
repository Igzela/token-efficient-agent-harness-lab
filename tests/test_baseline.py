import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import BaselineManager, EvalCase, EvaluationReport, RunScore


def make_report(suite_id="suite_1", cases=None, passed=1, failed=0):
    if cases is None:
        cases = (
            EvalCase(
                case_id="case_1",
                fixture_path=Path(""),
                expected_outcome="pass",
                actual_outcome="pass",
                passed=True,
            ),
        )
    return EvaluationReport(
        suite_id=suite_id,
        cases=cases,
        total=passed + failed,
        passed=passed,
        failed=failed,
    )


def make_score(aggregate=0.80, passed=1, failed=0):
    return RunScore(
        run_id="run_1",
        task_scores=(),
        aggregate_score=aggregate,
        grade="B",
        item_count=passed + failed,
        passed_count=passed,
        failed_count=failed,
    )


class BaselineManagerTests(unittest.TestCase):
    def test_save_baseline_writes_json(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            manager = BaselineManager(temp_dir)
            report = make_report()
            score = make_score()
            record = manager.save_baseline(report, score)
            path = Path(temp_dir) / f"{record.baseline_id}.json"
            self.assertTrue(path.exists())
            self.assertTrue(record.baseline_id.startswith("baseline_"))

    def test_load_latest_baseline(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            manager = BaselineManager(temp_dir)
            report = make_report()
            score = make_score()
            manager.save_baseline(report, score)
            loaded = manager.load_latest_baseline()
        self.assertIsNotNone(loaded)
        self.assertEqual(0.80, loaded.run_score.aggregate_score)

    def test_empty_baseline_dir_returns_none(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            manager = BaselineManager(Path(temp_dir) / "nonexistent")
            result = manager.load_latest_baseline()
        self.assertIsNone(result)

    def test_compare_no_regression(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            manager = BaselineManager(temp_dir)
            report = make_report()
            score = make_score()
            manager.save_baseline(report, score)

            current_report = make_report()
            current_score = make_score(aggregate=0.85)
            comparison = manager.compare(current_report, current_score)
        self.assertIsNotNone(comparison)
        self.assertFalse(comparison.regression_detected)
        self.assertGreater(comparison.score_delta, 0)

    def test_compare_score_regression(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            manager = BaselineManager(temp_dir)
            report = make_report()
            score = make_score(aggregate=0.80)
            manager.save_baseline(report, score)

            current_report = make_report()
            current_score = make_score(aggregate=0.50)
            comparison = manager.compare(current_report, current_score)
        self.assertIsNotNone(comparison)
        self.assertTrue(comparison.regression_detected or comparison.score_delta < 0)

    def test_compare_case_regression(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            manager = BaselineManager(temp_dir)
            baseline_cases = (
                EvalCase("c1", Path(""), "pass", "pass", True),
                EvalCase("c2", Path(""), "pass", "pass", True),
            )
            report = make_report(cases=baseline_cases, passed=2)
            score = make_score(aggregate=0.90, passed=2)
            manager.save_baseline(report, score)

            current_cases = (
                EvalCase("c1", Path(""), "pass", "pass", True),
                EvalCase("c2", Path(""), "pass", "fail", False),
            )
            current_report = make_report(cases=current_cases, passed=1, failed=1)
            current_score = make_score(aggregate=0.50, passed=1, failed=1)
            comparison = manager.compare(current_report, current_score)
        self.assertIsNotNone(comparison)
        self.assertTrue(comparison.regression_detected)
        self.assertIn("c2", comparison.regressed_cases)

    def test_compare_improvement(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            manager = BaselineManager(temp_dir)
            baseline_cases = (EvalCase("c1", Path(""), "pass", "fail", False),)
            report = make_report(cases=baseline_cases, passed=0, failed=1)
            score = make_score(aggregate=0.30, passed=0, failed=1)
            manager.save_baseline(report, score)

            current_cases = (EvalCase("c1", Path(""), "pass", "pass", True),)
            current_report = make_report(cases=current_cases, passed=1)
            current_score = make_score(aggregate=0.90, passed=1)
            comparison = manager.compare(current_report, current_score)
        self.assertIsNotNone(comparison)
        self.assertFalse(comparison.regression_detected)
        self.assertIn("c1", comparison.improved_cases)


if __name__ == "__main__":
    unittest.main()
