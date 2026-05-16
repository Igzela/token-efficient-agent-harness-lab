import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.model_gateway import create_default_gateway
from harness_core.sampling import SamplingCandidate, SamplingReport, SamplingRunner


def make_task_spec():
    return {
        "task_id": "task_001",
        "type": "code_small_change",
        "objective": "Implement a simple function",
    }


class SamplingRunnerTests(unittest.TestCase):
    def test_n_candidates_produced(self):
        runner = SamplingRunner(create_default_gateway())
        report = runner.run(make_task_spec(), n=5, tier="cheap_executor")
        self.assertEqual(5, len(report.candidates))

    def test_best_candidate_id_selected(self):
        runner = SamplingRunner(create_default_gateway())
        report = runner.run(make_task_spec(), n=3, tier="cheap_executor")
        candidate_ids = [c.candidate_id for c in report.candidates]
        self.assertIn(report.best_candidate_id, candidate_ids)

    def test_deterministic_repeatability(self):
        runner = SamplingRunner(create_default_gateway())
        r1 = runner.run(make_task_spec(), n=4, tier="strong_planner")
        r2 = runner.run(make_task_spec(), n=4, tier="strong_planner")
        self.assertEqual(r1.best_candidate_id, r2.best_candidate_id)
        self.assertEqual(r1.best_score, r2.best_score)
        self.assertEqual(len(r1.candidates), len(r2.candidates))

    def test_invalid_n_rejected(self):
        runner = SamplingRunner(create_default_gateway())
        with self.assertRaises(ValueError):
            runner.run(make_task_spec(), n=0, tier="cheap_executor")

    def test_negative_n_rejected(self):
        runner = SamplingRunner(create_default_gateway())
        with self.assertRaises(ValueError):
            runner.run(make_task_spec(), n=-1, tier="cheap_executor")

    def test_no_real_model_calls(self):
        runner = SamplingRunner(create_default_gateway())
        report = runner.run(make_task_spec(), n=2, tier="cheap_executor")
        for cand in report.candidates:
            self.assertIsInstance(cand, SamplingCandidate)
            self.assertEqual("cheap_executor", cand.tier)

    def test_best_score_non_negative(self):
        runner = SamplingRunner(create_default_gateway())
        report = runner.run(make_task_spec(), n=3, tier="verifier")
        self.assertGreaterEqual(report.best_score, 0.0)

    def test_task_id_in_report(self):
        runner = SamplingRunner(create_default_gateway())
        report = runner.run(make_task_spec(), n=2, tier="advisor")
        self.assertEqual("task_001", report.task_id)

    def test_one_candidate(self):
        runner = SamplingRunner(create_default_gateway())
        report = runner.run(make_task_spec(), n=1, tier="cheap_executor")
        self.assertEqual(1, len(report.candidates))
        self.assertEqual(report.best_candidate_id, report.candidates[0].candidate_id)

    def test_selection_method(self):
        runner = SamplingRunner(create_default_gateway())
        report = runner.run(make_task_spec(), n=3, tier="strong_planner")
        self.assertEqual("highest_score", report.selection_method)


if __name__ == "__main__":
    unittest.main()
