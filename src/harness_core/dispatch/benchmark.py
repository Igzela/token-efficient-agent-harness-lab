"""Phase 7: BenchmarkSuite — model comparison benchmarks and leaderboard."""

from __future__ import annotations

import threading
from collections import defaultdict
from dataclasses import dataclass


BENCHMARK_SCHEMA_VERSION = "benchmark.v1"


@dataclass(frozen=True)
class BenchmarkTask:
    task_id: str
    prompt: str
    expected_quality: float
    task_group: str
    max_tokens: int = 1000
    schema_version: str = BENCHMARK_SCHEMA_VERSION


@dataclass(frozen=True)
class BenchmarkResult:
    task_id: str
    model_name: str
    provider: str
    output: str
    quality_score: float
    tokens_used: int
    latency_ms: float
    cost_usd: float
    passed: bool
    schema_version: str = BENCHMARK_SCHEMA_VERSION


class BenchmarkSuite:
    def __init__(self) -> None:
        self._tasks: dict[str, BenchmarkTask] = {}
        self._results: dict[str, list[BenchmarkResult]] = defaultdict(list)
        self._lock = threading.Lock()

    def validate_task(self, task: BenchmarkTask) -> list[str]:
        errors: list[str] = []
        if not task.task_id:
            errors.append("task_id is required")
        if not task.prompt:
            errors.append("prompt is required")
        if not (0.0 <= task.expected_quality <= 1.0):
            errors.append("expected_quality must be between 0.0 and 1.0")
        if not task.task_group:
            errors.append("task_group is required")
        if task.max_tokens <= 0:
            errors.append("max_tokens must be positive")
        if task.schema_version != BENCHMARK_SCHEMA_VERSION:
            errors.append(f"schema_version must be {BENCHMARK_SCHEMA_VERSION}")
        return errors

    def validate_result(self, result: BenchmarkResult) -> list[str]:
        errors: list[str] = []
        if not result.task_id:
            errors.append("task_id is required")
        if not result.model_name:
            errors.append("model_name is required")
        if not result.provider:
            errors.append("provider is required")
        if not (0.0 <= result.quality_score <= 1.0):
            errors.append("quality_score must be between 0.0 and 1.0")
        if result.tokens_used < 0:
            errors.append("tokens_used must be non-negative")
        if result.latency_ms < 0:
            errors.append("latency_ms must be non-negative")
        if result.cost_usd < 0:
            errors.append("cost_usd must be non-negative")
        if result.schema_version != BENCHMARK_SCHEMA_VERSION:
            errors.append(f"schema_version must be {BENCHMARK_SCHEMA_VERSION}")
        return errors

    def add_task(self, task: BenchmarkTask) -> bool:
        errors = self.validate_task(task)
        if errors:
            return False
        with self._lock:
            if task.task_id in self._tasks:
                return False
            self._tasks[task.task_id] = task
            return True

    def remove_task(self, task_id: str) -> bool:
        with self._lock:
            if task_id not in self._tasks:
                return False
            del self._tasks[task_id]
            self._results.pop(task_id, None)
            return True

    def list_tasks(self) -> list[BenchmarkTask]:
        with self._lock:
            return list(self._tasks.values())

    def record_result(self, result: BenchmarkResult) -> bool:
        errors = self.validate_result(result)
        if errors:
            return False
        with self._lock:
            if result.task_id not in self._tasks:
                return False
            self._results[result.task_id].append(result)
            return True

    def results_for_model(self, model_name: str) -> list[BenchmarkResult]:
        with self._lock:
            return [
                r for results in self._results.values()
                for r in results
                if r.model_name == model_name
            ]

    def results_for_task(self, task_id: str) -> list[BenchmarkResult]:
        with self._lock:
            return list(self._results.get(task_id, []))

    def compare_models(self, model_a: str, model_b: str) -> dict:
        with self._lock:
            all_results = [
                r for results in self._results.values() for r in results
            ]
        a_results = [r for r in all_results if r.model_name == model_a]
        b_results = [r for r in all_results if r.model_name == model_b]

        def _stats(results: list[BenchmarkResult]) -> dict:
            if not results:
                return {
                    "avg_quality": 0.0,
                    "avg_latency": 0.0,
                    "avg_cost": 0.0,
                    "pass_rate": 0.0,
                    "task_count": 0,
                }
            n = len(results)
            return {
                "avg_quality": sum(r.quality_score for r in results) / n,
                "avg_latency": sum(r.latency_ms for r in results) / n,
                "avg_cost": sum(r.cost_usd for r in results) / n,
                "pass_rate": sum(1 for r in results if r.passed) / n,
                "task_count": n,
            }

        return {
            "model_a": model_a,
            "model_b": model_b,
            "model_a_stats": _stats(a_results),
            "model_b_stats": _stats(b_results),
        }

    def leaderboard(self) -> list[dict]:
        model_data: dict[str, list[BenchmarkResult]] = defaultdict(list)
        with self._lock:
            for results in self._results.values():
                for r in results:
                    model_data[r.model_name].append(r)

        entries: list[dict] = []
        for model, results in model_data.items():
            n = len(results)
            entries.append({
                "model": model,
                "avg_quality": sum(r.quality_score for r in results) / n,
                "avg_latency": sum(r.latency_ms for r in results) / n,
                "avg_cost": sum(r.cost_usd for r in results) / n,
                "pass_rate": sum(1 for r in results if r.passed) / n,
                "task_count": n,
            })

        entries.sort(key=lambda e: e["avg_quality"], reverse=True)
        return entries
