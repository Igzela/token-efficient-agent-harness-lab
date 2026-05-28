"""Phase 7: DispatchDashboard — experiment tracking and summary computation."""

from __future__ import annotations

import threading
import time
from collections import Counter
from dataclasses import dataclass


DASHBOARD_SCHEMA_VERSION = "dashboard.v1"

VALID_WINNERS = ("a", "b", "tie")


@dataclass(frozen=True)
class ExperimentResult:
    experiment_id: str
    model_a: str
    model_b: str
    task_group: str
    metric_name: str
    value_a: float
    value_b: float
    winner: str
    sample_count: int
    created_at: float
    schema_version: str = DASHBOARD_SCHEMA_VERSION


@dataclass(frozen=True)
class DashboardSummary:
    total_dispatches: int
    total_plans: int
    active_experiments: int
    cost_savings_pct: float
    quality_delta_pct: float
    top_models: tuple[tuple[str, int], ...]


class DispatchDashboard:
    def __init__(self) -> None:
        self._experiments: dict[str, ExperimentResult] = {}
        self._lock = threading.Lock()

    def validate_experiment(self, result: ExperimentResult) -> list[str]:
        errors: list[str] = []
        if not result.experiment_id:
            errors.append("experiment_id is required")
        if not result.model_a:
            errors.append("model_a is required")
        if not result.model_b:
            errors.append("model_b is required")
        if not result.task_group:
            errors.append("task_group is required")
        if not result.metric_name:
            errors.append("metric_name is required")
        if result.winner not in VALID_WINNERS:
            errors.append(f"winner must be one of {VALID_WINNERS}")
        if result.sample_count < 0:
            errors.append("sample_count must be non-negative")
        if result.schema_version != DASHBOARD_SCHEMA_VERSION:
            errors.append(f"schema_version must be {DASHBOARD_SCHEMA_VERSION}")
        return errors

    def record_experiment(self, result: ExperimentResult) -> bool:
        errors = self.validate_experiment(result)
        if errors:
            return False
        with self._lock:
            if result.experiment_id in self._experiments:
                return False
            self._experiments[result.experiment_id] = result
            return True

    def get_experiment(self, experiment_id: str) -> ExperimentResult | None:
        with self._lock:
            return self._experiments.get(experiment_id)

    def list_experiments(self) -> list[ExperimentResult]:
        with self._lock:
            return list(self._experiments.values())

    def experiments_by_model(self, model_name: str) -> list[ExperimentResult]:
        with self._lock:
            return [
                e for e in self._experiments.values()
                if e.model_a == model_name or e.model_b == model_name
            ]

    def experiments_by_task_group(self, task_group: str) -> list[ExperimentResult]:
        with self._lock:
            return [
                e for e in self._experiments.values()
                if e.task_group == task_group
            ]

    def compute_summary(
        self,
        total_dispatches: int = 0,
        total_plans: int = 0,
    ) -> DashboardSummary:
        with self._lock:
            experiments = list(self._experiments.values())

        active = len(experiments)

        cost_savings = 0.0
        quality_delta = 0.0
        model_counter: Counter[str] = Counter()

        for e in experiments:
            model_counter[e.model_a] += 1
            model_counter[e.model_b] += 1

            if e.value_a != 0:
                cost_savings += (e.value_b - e.value_a) / abs(e.value_a) * 100
            if e.value_a != 0:
                quality_delta += (e.value_b - e.value_a) / abs(e.value_a) * 100

        if active > 0:
            cost_savings /= active
            quality_delta /= active

        top = tuple(model_counter.most_common())

        return DashboardSummary(
            total_dispatches=total_dispatches,
            total_plans=total_plans,
            active_experiments=active,
            cost_savings_pct=cost_savings,
            quality_delta_pct=quality_delta,
            top_models=top,
        )
