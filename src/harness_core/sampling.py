"""Sampling Runner for Stage 3 — deterministic N-variant sampling."""

from __future__ import annotations

import logging
from dataclasses import dataclass

_log = logging.getLogger(__name__)
from typing import Any

from .model_gateway import ModelGateway
from .scoring import ScoringEngine, TaskScore


@dataclass(frozen=True)
class SamplingCandidate:
    candidate_id: str
    output: str
    tier: str


@dataclass(frozen=True)
class SamplingReport:
    task_id: str
    candidates: tuple[SamplingCandidate, ...]
    best_candidate_id: str
    best_score: float
    selection_method: str  # "highest_score"


class SamplingRunner:
    """Run N deterministic variants and select the best candidate."""

    def __init__(self, gateway: ModelGateway, scoring: ScoringEngine | None = None):
        self._gateway = gateway
        self._scoring = scoring or ScoringEngine()

    def run(
        self, task_spec: dict[str, Any], n: int, tier: str
    ) -> SamplingReport:
        if n <= 0:
            raise ValueError(f"n must be > 0, got {n}")

        task_id = task_spec.get("task_id", "unknown_task")
        candidates: list[SamplingCandidate] = []

        for i in range(n):
            prompt = self._build_prompt(task_spec, i)
            try:
                response = self._gateway.invoke(tier, prompt)
                content = response.content
            except Exception:
                _log.exception("sampling variant %d failed for task %s", i, task_id)
                content = f"[error: variant {i} failed]"

            candidates.append(
                SamplingCandidate(
                    candidate_id=f"{task_id}:variant_{i}",
                    output=content,
                    tier=tier,
                )
            )

        best_id, best_score = self._select_best(candidates, task_spec)

        return SamplingReport(
            task_id=task_id,
            candidates=tuple(candidates),
            best_candidate_id=best_id,
            best_score=best_score,
            selection_method="highest_score",
        )

    def _build_prompt(self, task_spec: dict[str, Any], variant_index: int) -> str:
        parts = [f"task_id: {task_spec.get('task_id', 'unknown')}"]
        parts.append(f"variant: {variant_index}")
        task_type = task_spec.get("type", "unknown")
        parts.append(f"type: {task_type}")
        objective = task_spec.get("objective", "")
        if objective:
            parts.append(f"objective: {objective}")
        return "; ".join(parts)

    def _select_best(
        self,
        candidates: list[SamplingCandidate],
        task_spec: dict[str, Any],
    ) -> tuple[str, float]:
        if not candidates:
            return "", 0.0

        scores: list[tuple[str, float]] = []
        for cand in candidates:
            score = self._score_candidate(cand, task_spec)
            scores.append((cand.candidate_id, score))

        scores.sort(key=lambda x: x[1], reverse=True)
        return scores[0]

    def _score_candidate(
        self, candidate: SamplingCandidate, task_spec: dict[str, Any]
    ) -> float:
        content = candidate.output
        if "[error:" in content:
            return 0.0

        score = 0.5
        if task_spec.get("type") in content.lower():
            score += 0.2
        if len(content) > 50:
            score += 0.1
        if task_spec.get("task_id", "unknown") in content:
            score += 0.1
        return min(1.0, score)
