"""Tests for dispatch/dashboard.py — Experiment tracking and summary computation."""

import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dashboard import (
    DASHBOARD_SCHEMA_VERSION,
    DashboardSummary,
    DispatchDashboard,
    ExperimentResult,
)


def _make_experiment(
    experiment_id: str = "exp-1",
    model_a: str = "model-a",
    model_b: str = "model-b",
    task_group: str = "group-1",
    metric_name: str = "quality",
    value_a: float = 0.8,
    value_b: float = 0.9,
    winner: str = "b",
    sample_count: int = 10,
    created_at: float = 0.0,
    schema_version: str = DASHBOARD_SCHEMA_VERSION,
) -> ExperimentResult:
    return ExperimentResult(
        experiment_id=experiment_id,
        model_a=model_a,
        model_b=model_b,
        task_group=task_group,
        metric_name=metric_name,
        value_a=value_a,
        value_b=value_b,
        winner=winner,
        sample_count=sample_count,
        created_at=created_at,
        schema_version=schema_version,
    )


class SchemaVersionTest(unittest.TestCase):
    def test_version_defined(self):
        self.assertEqual(DASHBOARD_SCHEMA_VERSION, "dashboard.v1")


class ExperimentResultTests(unittest.TestCase):
    def test_fields(self):
        r = _make_experiment()
        self.assertEqual(r.experiment_id, "exp-1")
        self.assertEqual(r.model_a, "model-a")
        self.assertEqual(r.model_b, "model-b")
        self.assertEqual(r.task_group, "group-1")
        self.assertEqual(r.metric_name, "quality")
        self.assertAlmostEqual(r.value_a, 0.8)
        self.assertAlmostEqual(r.value_b, 0.9)
        self.assertEqual(r.winner, "b")
        self.assertEqual(r.sample_count, 10)
        self.assertEqual(r.schema_version, DASHBOARD_SCHEMA_VERSION)

    def test_immutable(self):
        r = _make_experiment()
        with self.assertRaises(AttributeError):
            r.experiment_id = "changed"  # type: ignore[misc]

    def test_default_schema_version(self):
        r = _make_experiment()
        self.assertEqual(r.schema_version, DASHBOARD_SCHEMA_VERSION)


class DashboardSummaryTests(unittest.TestCase):
    def test_fields(self):
        s = DashboardSummary(
            total_dispatches=100,
            total_plans=50,
            active_experiments=5,
            cost_savings_pct=12.5,
            quality_delta_pct=3.2,
            top_models=(("gpt-4", 10), ("claude-3", 8)),
        )
        self.assertEqual(s.total_dispatches, 100)
        self.assertEqual(s.total_plans, 50)
        self.assertEqual(s.active_experiments, 5)
        self.assertAlmostEqual(s.cost_savings_pct, 12.5)
        self.assertEqual(len(s.top_models), 2)

    def test_immutable(self):
        s = DashboardSummary(
            total_dispatches=0,
            total_plans=0,
            active_experiments=0,
            cost_savings_pct=0.0,
            quality_delta_pct=0.0,
            top_models=(),
        )
        with self.assertRaises(AttributeError):
            s.total_dispatches = 1  # type: ignore[misc]


class RecordExperimentTests(unittest.TestCase):
    def setUp(self):
        self.dash = DispatchDashboard()

    def test_record_valid_experiment(self):
        r = _make_experiment()
        self.assertTrue(self.dash.record_experiment(r))
        self.assertEqual(len(self.dash.list_experiments()), 1)

    def test_reject_duplicate_experiment_id(self):
        r1 = _make_experiment(experiment_id="dup")
        r2 = _make_experiment(experiment_id="dup")
        self.assertTrue(self.dash.record_experiment(r1))
        self.assertFalse(self.dash.record_experiment(r2))
        self.assertEqual(len(self.dash.list_experiments()), 1)

    def test_reject_invalid_experiment(self):
        r = _make_experiment(experiment_id="")
        self.assertFalse(self.dash.record_experiment(r))
        self.assertEqual(len(self.dash.list_experiments()), 0)

    def test_record_multiple_experiments(self):
        for i in range(3):
            r = _make_experiment(experiment_id=f"exp-{i}")
            self.assertTrue(self.dash.record_experiment(r))
        self.assertEqual(len(self.dash.list_experiments()), 3)


class GetExperimentTests(unittest.TestCase):
    def setUp(self):
        self.dash = DispatchDashboard()

    def test_returns_existing_experiment(self):
        r = _make_experiment()
        self.dash.record_experiment(r)
        found = self.dash.get_experiment("exp-1")
        self.assertIsNotNone(found)
        self.assertEqual(found.experiment_id, "exp-1")

    def test_returns_none_for_missing(self):
        self.assertIsNone(self.dash.get_experiment("nonexistent"))

    def test_returns_correct_experiment(self):
        self.dash.record_experiment(_make_experiment(experiment_id="a"))
        self.dash.record_experiment(_make_experiment(experiment_id="b"))
        found = self.dash.get_experiment("b")
        self.assertIsNotNone(found)
        self.assertEqual(found.experiment_id, "b")


class ListExperimentsTests(unittest.TestCase):
    def setUp(self):
        self.dash = DispatchDashboard()

    def test_empty_when_no_experiments(self):
        self.assertEqual(self.dash.list_experiments(), [])

    def test_returns_all(self):
        for i in range(3):
            self.dash.record_experiment(_make_experiment(experiment_id=f"e-{i}"))
        self.assertEqual(len(self.dash.list_experiments()), 3)

    def test_returns_copy(self):
        self.dash.record_experiment(_make_experiment())
        result = self.dash.list_experiments()
        result.clear()
        self.assertEqual(len(self.dash.list_experiments()), 1)


class ExperimentsByModelTests(unittest.TestCase):
    def setUp(self):
        self.dash = DispatchDashboard()

    def test_finds_experiments_with_model_as_a(self):
        self.dash.record_experiment(_make_experiment(model_a="gpt-4"))
        results = self.dash.experiments_by_model("gpt-4")
        self.assertEqual(len(results), 1)

    def test_finds_experiments_with_model_as_b(self):
        self.dash.record_experiment(_make_experiment(model_b="gpt-4"))
        results = self.dash.experiments_by_model("gpt-4")
        self.assertEqual(len(results), 1)

    def test_finds_experiments_with_model_in_both(self):
        self.dash.record_experiment(_make_experiment(experiment_id="e1", model_a="gpt-4", model_b="other"))
        self.dash.record_experiment(_make_experiment(experiment_id="e2", model_a="other", model_b="gpt-4"))
        results = self.dash.experiments_by_model("gpt-4")
        self.assertEqual(len(results), 2)

    def test_returns_empty_for_unmatched_model(self):
        self.dash.record_experiment(_make_experiment(model_a="gpt-4", model_b="claude-3"))
        self.assertEqual(len(self.dash.experiments_by_model("unknown")), 0)


class ExperimentsByTaskGroupTests(unittest.TestCase):
    def setUp(self):
        self.dash = DispatchDashboard()

    def test_finds_experiments_in_group(self):
        self.dash.record_experiment(_make_experiment(task_group="code-review"))
        self.dash.record_experiment(_make_experiment(task_group="summarize"))
        results = self.dash.experiments_by_task_group("code-review")
        self.assertEqual(len(results), 1)

    def test_returns_empty_for_unknown_group(self):
        self.dash.record_experiment(_make_experiment(task_group="code-review"))
        self.assertEqual(len(self.dash.experiments_by_task_group("unknown")), 0)


class ComputeSummaryTests(unittest.TestCase):
    def setUp(self):
        self.dash = DispatchDashboard()

    def test_empty_summary(self):
        s = self.dash.compute_summary(total_dispatches=100, total_plans=50)
        self.assertEqual(s.total_dispatches, 100)
        self.assertEqual(s.total_plans, 50)
        self.assertEqual(s.active_experiments, 0)
        self.assertAlmostEqual(s.cost_savings_pct, 0.0)
        self.assertAlmostEqual(s.quality_delta_pct, 0.0)
        self.assertEqual(s.top_models, ())

    def test_single_experiment_summary(self):
        self.dash.record_experiment(_make_experiment(
            model_a="gpt-4", model_b="claude-3",
            value_a=10.0, value_b=8.0, metric_name="quality",
        ))
        s = self.dash.compute_summary()
        self.assertEqual(s.active_experiments, 1)
        self.assertAlmostEqual(s.cost_savings_pct, 0.0)
        self.assertAlmostEqual(s.quality_delta_pct, -20.0)
        self.assertEqual(len(s.top_models), 2)

    def test_multiple_experiments_averages(self):
        self.dash.record_experiment(_make_experiment(
            experiment_id="e1", value_a=100.0, value_b=80.0,
        ))
        self.dash.record_experiment(_make_experiment(
            experiment_id="e2", value_a=100.0, value_b=120.0,
        ))
        s = self.dash.compute_summary()
        self.assertEqual(s.active_experiments, 2)
        self.assertAlmostEqual(s.cost_savings_pct, 0.0)

    def test_top_models_ranked_by_count(self):
        self.dash.record_experiment(_make_experiment(
            experiment_id="e1", model_a="gpt-4", model_b="claude-3",
        ))
        self.dash.record_experiment(_make_experiment(
            experiment_id="e2", model_a="gpt-4", model_b="gemini",
        ))
        self.dash.record_experiment(_make_experiment(
            experiment_id="e3", model_a="gpt-4", model_b="claude-3",
        ))
        s = self.dash.compute_summary()
        self.assertEqual(s.top_models[0][0], "gpt-4")
        self.assertEqual(s.top_models[0][1], 3)

    def test_summary_with_zero_value_a(self):
        self.dash.record_experiment(_make_experiment(
            value_a=0.0, value_b=5.0,
        ))
        s = self.dash.compute_summary()
        self.assertAlmostEqual(s.cost_savings_pct, 0.0)
        self.assertAlmostEqual(s.quality_delta_pct, 0.0)


class ValidationTests(unittest.TestCase):
    def setUp(self):
        self.dash = DispatchDashboard()

    def test_valid_experiment_no_errors(self):
        errors = self.dash.validate_experiment(_make_experiment())
        self.assertEqual(errors, [])

    def test_empty_experiment_id(self):
        errors = self.dash.validate_experiment(_make_experiment(experiment_id=""))
        self.assertTrue(any("experiment_id" in e for e in errors))

    def test_empty_model_a(self):
        errors = self.dash.validate_experiment(_make_experiment(model_a=""))
        self.assertTrue(any("model_a" in e for e in errors))

    def test_empty_model_b(self):
        errors = self.dash.validate_experiment(_make_experiment(model_b=""))
        self.assertTrue(any("model_b" in e for e in errors))

    def test_empty_task_group(self):
        errors = self.dash.validate_experiment(_make_experiment(task_group=""))
        self.assertTrue(any("task_group" in e for e in errors))

    def test_empty_metric_name(self):
        errors = self.dash.validate_experiment(_make_experiment(metric_name=""))
        self.assertTrue(any("metric_name" in e for e in errors))

    def test_invalid_winner(self):
        errors = self.dash.validate_experiment(_make_experiment(winner="c"))
        self.assertTrue(any("winner" in e for e in errors))

    def test_negative_sample_count(self):
        errors = self.dash.validate_experiment(_make_experiment(sample_count=-1))
        self.assertTrue(any("sample_count" in e for e in errors))

    def test_wrong_schema_version(self):
        errors = self.dash.validate_experiment(_make_experiment(schema_version="old.v0"))
        self.assertTrue(any("schema_version" in e for e in errors))

    def test_multiple_errors(self):
        errors = self.dash.validate_experiment(
            _make_experiment(experiment_id="", model_a="", winner="x")
        )
        self.assertGreaterEqual(len(errors), 3)


class MixedMetricSummaryTests(unittest.TestCase):
    def setUp(self):
        self.dash = DispatchDashboard()

    def test_cost_and_quality_experiments_separated(self):
        self.dash.record_experiment(_make_experiment(
            experiment_id="cost-1", metric_name="cost",
            value_a=100.0, value_b=80.0,
        ))
        self.dash.record_experiment(_make_experiment(
            experiment_id="quality-1", metric_name="quality",
            value_a=0.7, value_b=0.9,
        ))
        s = self.dash.compute_summary()
        self.assertEqual(s.active_experiments, 2)
        self.assertAlmostEqual(s.cost_savings_pct, -10.0)
        self.assertAlmostEqual(s.quality_delta_pct, 14.285714, places=4)

    def test_unknown_metric_ignored(self):
        self.dash.record_experiment(_make_experiment(
            experiment_id="unknown-1", metric_name="latency",
            value_a=100.0, value_b=50.0,
        ))
        s = self.dash.compute_summary()
        self.assertAlmostEqual(s.cost_savings_pct, 0.0)
        self.assertAlmostEqual(s.quality_delta_pct, 0.0)


class NaNInfValidationTests(unittest.TestCase):
    def setUp(self):
        self.dash = DispatchDashboard()

    def test_reject_nan_value_a(self):
        import math
        r = _make_experiment(value_a=float("nan"))
        errors = self.dash.validate_experiment(r)
        self.assertTrue(any("finite" in e for e in errors))

    def test_reject_inf_value_b(self):
        r = _make_experiment(value_b=float("inf"))
        errors = self.dash.validate_experiment(r)
        self.assertTrue(any("finite" in e for e in errors))

    def test_reject_negative_inf(self):
        r = _make_experiment(value_a=float("-inf"))
        errors = self.dash.validate_experiment(r)
        self.assertTrue(any("finite" in e for e in errors))

    def test_accept_finite_values(self):
        r = _make_experiment(value_a=0.0, value_b=1.0)
        errors = self.dash.validate_experiment(r)
        self.assertFalse(any("finite" in e for e in errors))


class ThreadSafetyTest(unittest.TestCase):
    def test_concurrent_record_experiment(self):
        dash = DispatchDashboard()
        results = []

        def record_one(i: int) -> None:
            r = _make_experiment(experiment_id=f"exp-{i}")
            ok = dash.record_experiment(r)
            results.append(ok)

        threads = [threading.Thread(target=record_one, args=(i,)) for i in range(20)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        self.assertEqual(len(results), 20)
        self.assertTrue(all(results))
        self.assertEqual(len(dash.list_experiments()), 20)

    def test_concurrent_read_write(self):
        dash = DispatchDashboard()
        for i in range(10):
            dash.record_experiment(_make_experiment(experiment_id=f"init-{i}"))

        errors: list[Exception] = []

        def writer() -> None:
            try:
                for i in range(10):
                    r = _make_experiment(experiment_id=f"new-{i}")
                    dash.record_experiment(r)
            except Exception as e:
                errors.append(e)

        def reader() -> None:
            try:
                for _ in range(10):
                    dash.list_experiments()
                    dash.get_experiment("init-5")
            except Exception as e:
                errors.append(e)

        threads = [threading.Thread(target=writer) for _ in range(3)]
        threads += [threading.Thread(target=reader) for _ in range(3)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        self.assertEqual(errors, [])


if __name__ == "__main__":
    unittest.main()
