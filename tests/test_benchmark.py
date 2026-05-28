"""Tests for dispatch/benchmark.py — Model comparison benchmarks and leaderboard."""

import sys
import threading
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.benchmark import (
    BENCHMARK_SCHEMA_VERSION,
    BenchmarkResult,
    BenchmarkSuite,
    BenchmarkTask,
)


def _make_task(
    task_id: str = "task-1",
    prompt: str = "Do something",
    expected_quality: float = 0.8,
    task_group: str = "code-review",
    max_tokens: int = 1000,
    schema_version: str = BENCHMARK_SCHEMA_VERSION,
) -> BenchmarkTask:
    return BenchmarkTask(
        task_id=task_id,
        prompt=prompt,
        expected_quality=expected_quality,
        task_group=task_group,
        max_tokens=max_tokens,
        schema_version=schema_version,
    )


def _make_result(
    task_id: str = "task-1",
    model_name: str = "gpt-4",
    provider: str = "openai",
    output: str = "result text",
    quality_score: float = 0.85,
    tokens_used: int = 500,
    latency_ms: float = 1200.0,
    cost_usd: float = 0.02,
    passed: bool = True,
    schema_version: str = BENCHMARK_SCHEMA_VERSION,
) -> BenchmarkResult:
    return BenchmarkResult(
        task_id=task_id,
        model_name=model_name,
        provider=provider,
        output=output,
        quality_score=quality_score,
        tokens_used=tokens_used,
        latency_ms=latency_ms,
        cost_usd=cost_usd,
        passed=passed,
        schema_version=schema_version,
    )


class SchemaVersionTest(unittest.TestCase):
    def test_version_defined(self):
        self.assertEqual(BENCHMARK_SCHEMA_VERSION, "benchmark.v1")


class BenchmarkTaskTests(unittest.TestCase):
    def test_fields(self):
        t = _make_task()
        self.assertEqual(t.task_id, "task-1")
        self.assertEqual(t.prompt, "Do something")
        self.assertAlmostEqual(t.expected_quality, 0.8)
        self.assertEqual(t.task_group, "code-review")
        self.assertEqual(t.max_tokens, 1000)
        self.assertEqual(t.schema_version, BENCHMARK_SCHEMA_VERSION)

    def test_immutable(self):
        t = _make_task()
        with self.assertRaises(AttributeError):
            t.task_id = "changed"  # type: ignore[misc]

    def test_default_max_tokens(self):
        t = _make_task(max_tokens=1000)
        self.assertEqual(t.max_tokens, 1000)


class BenchmarkResultTests(unittest.TestCase):
    def test_fields(self):
        r = _make_result()
        self.assertEqual(r.task_id, "task-1")
        self.assertEqual(r.model_name, "gpt-4")
        self.assertEqual(r.provider, "openai")
        self.assertEqual(r.output, "result text")
        self.assertAlmostEqual(r.quality_score, 0.85)
        self.assertEqual(r.tokens_used, 500)
        self.assertAlmostEqual(r.latency_ms, 1200.0)
        self.assertAlmostEqual(r.cost_usd, 0.02)
        self.assertTrue(r.passed)

    def test_immutable(self):
        r = _make_result()
        with self.assertRaises(AttributeError):
            r.model_name = "changed"  # type: ignore[misc]

    def test_default_schema_version(self):
        r = _make_result()
        self.assertEqual(r.schema_version, BENCHMARK_SCHEMA_VERSION)


class AddTaskTests(unittest.TestCase):
    def setUp(self):
        self.suite = BenchmarkSuite()

    def test_add_valid_task(self):
        self.assertTrue(self.suite.add_task(_make_task()))
        self.assertEqual(len(self.suite.list_tasks()), 1)

    def test_reject_duplicate_task_id(self):
        self.assertTrue(self.suite.add_task(_make_task(task_id="dup")))
        self.assertFalse(self.suite.add_task(_make_task(task_id="dup")))
        self.assertEqual(len(self.suite.list_tasks()), 1)

    def test_reject_invalid_task(self):
        self.assertFalse(self.suite.add_task(_make_task(task_id="")))
        self.assertEqual(len(self.suite.list_tasks()), 0)

    def test_add_multiple_tasks(self):
        for i in range(3):
            self.assertTrue(self.suite.add_task(_make_task(task_id=f"t-{i}")))
        self.assertEqual(len(self.suite.list_tasks()), 3)


class RemoveTaskTests(unittest.TestCase):
    def setUp(self):
        self.suite = BenchmarkSuite()

    def test_remove_existing_task(self):
        self.suite.add_task(_make_task())
        self.assertTrue(self.suite.remove_task("task-1"))
        self.assertEqual(len(self.suite.list_tasks()), 0)

    def test_remove_nonexistent_returns_false(self):
        self.assertFalse(self.suite.remove_task("no-such"))

    def test_remove_cascades_results(self):
        self.suite.add_task(_make_task())
        self.suite.record_result(_make_result())
        self.assertTrue(self.suite.remove_task("task-1"))
        self.assertEqual(len(self.suite.results_for_task("task-1")), 0)


class ListTasksTests(unittest.TestCase):
    def setUp(self):
        self.suite = BenchmarkSuite()

    def test_empty_when_no_tasks(self):
        self.assertEqual(self.suite.list_tasks(), [])

    def test_returns_all(self):
        self.suite.add_task(_make_task(task_id="a"))
        self.suite.add_task(_make_task(task_id="b"))
        self.assertEqual(len(self.suite.list_tasks()), 2)

    def test_returns_copy(self):
        self.suite.add_task(_make_task())
        tasks = self.suite.list_tasks()
        tasks.clear()
        self.assertEqual(len(self.suite.list_tasks()), 1)


class RecordResultTests(unittest.TestCase):
    def setUp(self):
        self.suite = BenchmarkSuite()
        self.suite.add_task(_make_task())

    def test_record_valid_result(self):
        self.assertTrue(self.suite.record_result(_make_result()))
        self.assertEqual(len(self.suite.results_for_task("task-1")), 1)

    def test_reject_invalid_result(self):
        self.assertFalse(self.suite.record_result(_make_result(model_name="")))
        self.assertEqual(len(self.suite.results_for_task("task-1")), 0)

    def test_reject_result_for_unregistered_task(self):
        self.assertFalse(self.suite.record_result(_make_result(task_id="nonexistent")))
        self.assertEqual(len(self.suite.results_for_task("nonexistent")), 0)

    def test_multiple_results_same_task(self):
        self.suite.record_result(_make_result(model_name="gpt-4"))
        self.suite.record_result(_make_result(model_name="claude-3"))
        self.assertEqual(len(self.suite.results_for_task("task-1")), 2)


class ResultsForModelTests(unittest.TestCase):
    def setUp(self):
        self.suite = BenchmarkSuite()

    def test_finds_results_by_model(self):
        self.suite.add_task(_make_task())
        self.suite.record_result(_make_result(model_name="gpt-4"))
        self.suite.record_result(_make_result(model_name="claude-3"))
        results = self.suite.results_for_model("gpt-4")
        self.assertEqual(len(results), 1)
        self.assertEqual(results[0].model_name, "gpt-4")

    def test_returns_empty_for_unknown_model(self):
        self.suite.add_task(_make_task())
        self.suite.record_result(_make_result(model_name="gpt-4"))
        self.assertEqual(len(self.suite.results_for_model("unknown")), 0)

    def test_across_multiple_tasks(self):
        self.suite.add_task(_make_task(task_id="t1"))
        self.suite.add_task(_make_task(task_id="t2"))
        self.suite.record_result(_make_result(task_id="t1", model_name="gpt-4"))
        self.suite.record_result(_make_result(task_id="t2", model_name="gpt-4"))
        self.suite.record_result(_make_result(task_id="t1", model_name="claude-3"))
        self.assertEqual(len(self.suite.results_for_model("gpt-4")), 2)
        self.assertEqual(len(self.suite.results_for_model("claude-3")), 1)


class ResultsForTaskTests(unittest.TestCase):
    def setUp(self):
        self.suite = BenchmarkSuite()
        self.suite.add_task(_make_task())

    def test_finds_results_for_task(self):
        self.suite.record_result(_make_result(model_name="gpt-4"))
        self.suite.record_result(_make_result(model_name="claude-3"))
        results = self.suite.results_for_task("task-1")
        self.assertEqual(len(results), 2)

    def test_returns_empty_for_unknown_task(self):
        self.assertEqual(len(self.suite.results_for_task("no-such")), 0)


class CompareModelsTests(unittest.TestCase):
    def setUp(self):
        self.suite = BenchmarkSuite()

    def test_compare_with_results(self):
        self.suite.add_task(_make_task())
        self.suite.record_result(_make_result(
            model_name="gpt-4", quality_score=0.9, latency_ms=1000.0, cost_usd=0.03, passed=True,
        ))
        self.suite.record_result(_make_result(
            model_name="claude-3", quality_score=0.8, latency_ms=1500.0, cost_usd=0.02, passed=True,
        ))
        comparison = self.suite.compare_models("gpt-4", "claude-3")
        self.assertEqual(comparison["model_a"], "gpt-4")
        self.assertEqual(comparison["model_b"], "claude-3")
        self.assertAlmostEqual(comparison["model_a_stats"]["avg_quality"], 0.9)
        self.assertAlmostEqual(comparison["model_b_stats"]["avg_quality"], 0.8)
        self.assertEqual(comparison["model_a_stats"]["task_count"], 1)
        self.assertEqual(comparison["model_b_stats"]["task_count"], 1)

    def test_compare_empty_models(self):
        comparison = self.suite.compare_models("a", "b")
        self.assertEqual(comparison["model_a_stats"]["task_count"], 0)
        self.assertEqual(comparison["model_b_stats"]["task_count"], 0)

    def test_compare_pass_rate(self):
        self.suite.add_task(_make_task())
        self.suite.record_result(_make_result(model_name="m1", passed=True))
        self.suite.record_result(_make_result(model_name="m1", passed=False))
        self.suite.record_result(_make_result(model_name="m2", passed=True))
        comparison = self.suite.compare_models("m1", "m2")
        self.assertAlmostEqual(comparison["model_a_stats"]["pass_rate"], 0.5)
        self.assertAlmostEqual(comparison["model_b_stats"]["pass_rate"], 1.0)

    def test_compare_one_model_absent(self):
        self.suite.add_task(_make_task())
        self.suite.record_result(_make_result(model_name="gpt-4"))
        comparison = self.suite.compare_models("gpt-4", "nonexistent")
        self.assertEqual(comparison["model_a_stats"]["task_count"], 1)
        self.assertEqual(comparison["model_b_stats"]["task_count"], 0)


class LeaderboardTests(unittest.TestCase):
    def setUp(self):
        self.suite = BenchmarkSuite()

    def test_empty_leaderboard(self):
        self.assertEqual(self.suite.leaderboard(), [])

    def test_ranked_by_quality(self):
        self.suite.add_task(_make_task())
        self.suite.record_result(_make_result(model_name="model-low", quality_score=0.5))
        self.suite.record_result(_make_result(model_name="model-high", quality_score=0.95))
        self.suite.record_result(_make_result(model_name="model-mid", quality_score=0.7))
        board = self.suite.leaderboard()
        self.assertEqual(len(board), 3)
        self.assertEqual(board[0]["model"], "model-high")
        self.assertEqual(board[1]["model"], "model-mid")
        self.assertEqual(board[2]["model"], "model-low")

    def test_leaderboard_entry_fields(self):
        self.suite.add_task(_make_task())
        self.suite.record_result(_make_result(
            model_name="gpt-4", quality_score=0.8, latency_ms=500.0, cost_usd=0.01, passed=True,
        ))
        self.suite.record_result(_make_result(
            model_name="gpt-4", quality_score=0.9, latency_ms=600.0, cost_usd=0.02, passed=True,
        ))
        board = self.suite.leaderboard()
        entry = board[0]
        self.assertEqual(entry["model"], "gpt-4")
        self.assertAlmostEqual(entry["avg_quality"], 0.85)
        self.assertAlmostEqual(entry["avg_latency"], 550.0)
        self.assertAlmostEqual(entry["avg_cost"], 0.015)
        self.assertAlmostEqual(entry["pass_rate"], 1.0)
        self.assertEqual(entry["task_count"], 2)

    def test_leaderboard_averages_multiple_models(self):
        self.suite.add_task(_make_task())
        for i in range(3):
            self.suite.record_result(_make_result(
                model_name="gpt-4", quality_score=0.8 + i * 0.05,
            ))
        self.suite.record_result(_make_result(
            model_name="claude-3", quality_score=0.85,
        ))
        board = self.suite.leaderboard()
        gpt4 = [e for e in board if e["model"] == "gpt-4"][0]
        claude = [e for e in board if e["model"] == "claude-3"][0]
        self.assertEqual(gpt4["task_count"], 3)
        self.assertEqual(claude["task_count"], 1)


class ValidateTaskTests(unittest.TestCase):
    def setUp(self):
        self.suite = BenchmarkSuite()

    def test_valid_task(self):
        errors = self.suite.validate_task(_make_task())
        self.assertEqual(errors, [])

    def test_empty_task_id(self):
        errors = self.suite.validate_task(_make_task(task_id=""))
        self.assertTrue(any("task_id" in e for e in errors))

    def test_empty_prompt(self):
        errors = self.suite.validate_task(_make_task(prompt=""))
        self.assertTrue(any("prompt" in e for e in errors))

    def test_quality_out_of_range_high(self):
        errors = self.suite.validate_task(_make_task(expected_quality=1.5))
        self.assertTrue(any("expected_quality" in e for e in errors))

    def test_quality_out_of_range_low(self):
        errors = self.suite.validate_task(_make_task(expected_quality=-0.1))
        self.assertTrue(any("expected_quality" in e for e in errors))

    def test_empty_task_group(self):
        errors = self.suite.validate_task(_make_task(task_group=""))
        self.assertTrue(any("task_group" in e for e in errors))

    def test_zero_max_tokens(self):
        errors = self.suite.validate_task(_make_task(max_tokens=0))
        self.assertTrue(any("max_tokens" in e for e in errors))

    def test_negative_max_tokens(self):
        errors = self.suite.validate_task(_make_task(max_tokens=-1))
        self.assertTrue(any("max_tokens" in e for e in errors))

    def test_wrong_schema_version(self):
        errors = self.suite.validate_task(_make_task(schema_version="old.v0"))
        self.assertTrue(any("schema_version" in e for e in errors))


class ValidateResultTests(unittest.TestCase):
    def setUp(self):
        self.suite = BenchmarkSuite()

    def test_valid_result(self):
        errors = self.suite.validate_result(_make_result())
        self.assertEqual(errors, [])

    def test_empty_task_id(self):
        errors = self.suite.validate_result(_make_result(task_id=""))
        self.assertTrue(any("task_id" in e for e in errors))

    def test_empty_model_name(self):
        errors = self.suite.validate_result(_make_result(model_name=""))
        self.assertTrue(any("model_name" in e for e in errors))

    def test_empty_provider(self):
        errors = self.suite.validate_result(_make_result(provider=""))
        self.assertTrue(any("provider" in e for e in errors))

    def test_quality_score_out_of_range(self):
        errors = self.suite.validate_result(_make_result(quality_score=2.0))
        self.assertTrue(any("quality_score" in e for e in errors))

    def test_negative_tokens_used(self):
        errors = self.suite.validate_result(_make_result(tokens_used=-1))
        self.assertTrue(any("tokens_used" in e for e in errors))

    def test_negative_latency_ms(self):
        errors = self.suite.validate_result(_make_result(latency_ms=-1.0))
        self.assertTrue(any("latency_ms" in e for e in errors))

    def test_negative_cost_usd(self):
        errors = self.suite.validate_result(_make_result(cost_usd=-0.01))
        self.assertTrue(any("cost_usd" in e for e in errors))

    def test_wrong_schema_version(self):
        errors = self.suite.validate_result(_make_result(schema_version="old"))
        self.assertTrue(any("schema_version" in e for e in errors))


class ThreadSafetyTest(unittest.TestCase):
    def test_concurrent_add_task(self):
        suite = BenchmarkSuite()
        results = []

        def add_one(i: int) -> None:
            ok = suite.add_task(_make_task(task_id=f"t-{i}"))
            results.append(ok)

        threads = [threading.Thread(target=add_one, args=(i,)) for i in range(20)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        self.assertEqual(len(results), 20)
        self.assertTrue(all(results))
        self.assertEqual(len(suite.list_tasks()), 20)

    def test_concurrent_record_result(self):
        suite = BenchmarkSuite()
        for i in range(5):
            suite.add_task(_make_task(task_id=f"t-{i}"))
        results = []

        def record_one(i: int) -> None:
            ok = suite.record_result(_make_result(task_id=f"t-{i % 5}"))
            results.append(ok)

        threads = [threading.Thread(target=record_one, args=(i,)) for i in range(20)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        self.assertEqual(len(results), 20)
        self.assertTrue(all(results))

    def test_concurrent_read_write(self):
        suite = BenchmarkSuite()
        for i in range(10):
            suite.add_task(_make_task(task_id=f"init-{i}"))

        errors: list[Exception] = []

        def writer() -> None:
            try:
                for i in range(10):
                    suite.add_task(_make_task(task_id=f"new-{i}"))
            except Exception as e:
                errors.append(e)

        def reader() -> None:
            try:
                for _ in range(10):
                    suite.list_tasks()
                    suite.leaderboard()
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
