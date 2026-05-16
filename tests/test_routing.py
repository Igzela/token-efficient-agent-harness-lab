import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.evaluation import EvalSpec, EvaluationReport, EvaluationRunner
from harness_core.routing import (
    RoutingExperimentManager,
    RoutingExperimentReport,
    RoutingExperimentSpec,
    RoutingExperimentResult,
    RoutingPolicy,
)
from harness_core.scoring import ScoringEngine


FIXTURE_DIR = Path(__file__).resolve().parents[1] / "docs" / "stage0"
EVENTS_FILE = FIXTURE_DIR / "events.jsonl"


def make_policy(policy_id, tier_map=None):
    if tier_map is None:
        tier_map = {
            "code_small_change": "cheap_executor",
            "bugfix": "cheap_executor",
            "doc_update": "cheap_executor",
        }
    return RoutingPolicy(
        policy_id=policy_id,
        tier_map=tier_map,
        description=f"Policy {policy_id}",
    )


def make_eval_cases():
    return (
        EvalSpec(
            case_id="case_1",
            fixture_path=EVENTS_FILE,
            expected_outcome="pass",
        ),
        EvalSpec(
            case_id="case_2",
            fixture_path=EVENTS_FILE,
            expected_outcome="pass",
        ),
        EvalSpec(
            case_id="case_3",
            fixture_path=EVENTS_FILE,
            expected_outcome="pass",
        ),
    )


class RoutingExperimentManagerTests(unittest.TestCase):
    def test_compares_two_policies(self):
        p1 = make_policy("baseline")
        p2 = make_policy("strong_everywhere", {"code_small_change": "strong_planner"})
        spec = RoutingExperimentSpec(
            experiment_id="exp_001",
            policies=(p1, p2),
            eval_cases=make_eval_cases(),
            description="test experiment",
        )
        manager = RoutingExperimentManager()
        report = manager.run_experiment(spec)
        self.assertIsInstance(report, RoutingExperimentReport)
        self.assertEqual(2, len(report.results))

    def test_produces_best_policy_id(self):
        p1 = make_policy("policy_a")
        p2 = make_policy("policy_b")
        spec = RoutingExperimentSpec(
            experiment_id="exp_002",
            policies=(p1, p2),
            eval_cases=make_eval_cases(),
            description="best policy test",
        )
        report = RoutingExperimentManager().run_experiment(spec)
        self.assertIn(report.best_policy_id, ("policy_a", "policy_b"))

    def test_no_automatic_routing_mutation(self):
        p1 = make_policy("baseline")
        spec = RoutingExperimentSpec(
            experiment_id="exp_003",
            policies=(p1,),
            eval_cases=make_eval_cases(),
            description="mutation test",
        )
        report = RoutingExperimentManager().run_experiment(spec)
        # The report should not contain any apply/commit/merge action
        self.assertNotIn("apply", report.recommendation)
        self.assertIsInstance(report, RoutingExperimentReport)

    def test_deterministic_result(self):
        p1 = make_policy("baseline")
        p2 = make_policy("alternative")
        spec = RoutingExperimentSpec(
            experiment_id="exp_det",
            policies=(p1, p2),
            eval_cases=make_eval_cases(),
            description="deterministic test",
        )
        manager = RoutingExperimentManager()
        r1 = manager.run_experiment(spec)
        r2 = manager.run_experiment(spec)
        self.assertEqual(r1.best_policy_id, r2.best_policy_id)
        self.assertEqual(r1.score_delta, r2.score_delta)
        self.assertEqual(r1.recommendation, r2.recommendation)

    def test_recommendation_needs_more_data_when_few_cases(self):
        p1 = make_policy("baseline")
        p2 = make_policy("alt")
        spec = RoutingExperimentSpec(
            experiment_id="exp_few",
            policies=(p1, p2),
            eval_cases=make_eval_cases()[:1],  # only 1 case
            description="few cases test",
        )
        report = RoutingExperimentManager().run_experiment(spec)
        self.assertEqual("needs_more_data", report.recommendation)

    def test_single_policy_needs_more_data(self):
        p1 = make_policy("only_one")
        spec = RoutingExperimentSpec(
            experiment_id="exp_single",
            policies=(p1,),
            eval_cases=make_eval_cases(),
            description="single policy test",
        )
        report = RoutingExperimentManager().run_experiment(spec)
        self.assertEqual("needs_more_data", report.recommendation)

    def test_each_result_has_score(self):
        p1 = make_policy("policy_a")
        p2 = make_policy("policy_b")
        spec = RoutingExperimentSpec(
            experiment_id="exp_scores",
            policies=(p1, p2),
            eval_cases=make_eval_cases(),
            description="score test",
        )
        report = RoutingExperimentManager().run_experiment(spec)
        for result in report.results:
            self.assertIsInstance(result, RoutingExperimentResult)
            self.assertGreaterEqual(result.run_score.aggregate_score, 0.0)


if __name__ == "__main__":
    unittest.main()
