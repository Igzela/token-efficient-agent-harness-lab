"""Final Gate runtime skeleton for Stage 1 Week 4."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .task_records import TaskRecordBundle
from .validators import (
    validate_approval_request,
    validate_completion_record,
    validate_handoff_pack,
)


@dataclass(frozen=True)
class FinalGateDecision:
    result: str
    next_project_status: str
    reasons: tuple[str, ...]
    evidence_refs: tuple[str, ...]


class FinalGateRunner:
    """Evaluate task completion evidence without mutating project state."""

    def evaluate(
        self,
        bundle: TaskRecordBundle,
        current_item_status: str,
    ) -> FinalGateDecision:
        reasons: list[str] = []
        evidence_refs = _collect_evidence_refs(bundle)

        if current_item_status != "review":
            return FinalGateDecision(
                result="fail",
                next_project_status="review",
                reasons=(f"project item must be review before Final Gate, got {current_item_status}",),
                evidence_refs=evidence_refs,
            )

        completion_result = validate_completion_record(bundle.completion)
        if not completion_result.ok:
            return _failed_review(
                reasons=tuple(f"completion.json: {error}" for error in completion_result.errors),
                evidence_refs=evidence_refs,
            )

        handoff_result = validate_handoff_pack(bundle.handoff_pack)
        if not handoff_result.ok:
            return _failed_review(
                reasons=tuple(f"handoff_pack.json: {error}" for error in handoff_result.errors),
                evidence_refs=evidence_refs,
            )

        approval_block = _pending_approval_reason(bundle)
        if approval_block is not None:
            return FinalGateDecision(
                result="fail",
                next_project_status="review",
                reasons=(approval_block,),
                evidence_refs=evidence_refs,
            )

        if bundle.completion.get("status") != "completed" or bundle.completion.get("exit_code") != 0:
            return FinalGateDecision(
                result="fail",
                next_project_status="failed",
                reasons=("task completion did not report completed with exit_code 0",),
                evidence_refs=evidence_refs,
            )

        warnings = []
        if bundle.run_log_path is None:
            warnings.append("run_log.md not present; treating task record as pass_with_notes")
        warnings.extend(completion_result.warnings)
        warnings.extend(handoff_result.warnings)
        if warnings:
            return FinalGateDecision(
                result="pass_with_notes",
                next_project_status="review",
                reasons=tuple(warnings),
                evidence_refs=evidence_refs,
            )

        return FinalGateDecision(
            result="pass",
            next_project_status="done",
            reasons=("completion and handoff evidence passed Final Gate",),
            evidence_refs=evidence_refs,
        )


def _failed_review(reasons: tuple[str, ...], evidence_refs: tuple[str, ...]) -> FinalGateDecision:
    return FinalGateDecision(
        result="fail",
        next_project_status="review",
        reasons=reasons,
        evidence_refs=evidence_refs,
    )


def _collect_evidence_refs(bundle: TaskRecordBundle) -> tuple[str, ...]:
    refs: list[str] = []
    for ref in bundle.handoff_pack.get("evidence_refs", []):
        if isinstance(ref, dict) and ref.get("path"):
            refs.append(str(ref["path"]))
        elif isinstance(ref, str):
            refs.append(ref)
    if bundle.run_log_path is not None:
        refs.append(str(bundle.run_log_path))
    return tuple(refs)


def _pending_approval_reason(bundle: TaskRecordBundle) -> str | None:
    for request in _walk_approval_requests(bundle.handoff_pack):
        result = validate_approval_request(request)
        if result.ok and request.get("decision") == "pending":
            approval_id = request.get("approval_id", "<unknown>")
            return f"approval_request {approval_id} is pending; Final Gate did not execute approval"
    return None


def _walk_approval_requests(value: Any):
    if isinstance(value, dict):
        if "approval_id" in value and "decision" in value:
            yield value
        for child in value.values():
            yield from _walk_approval_requests(child)
    elif isinstance(value, list):
        for child in value:
            yield from _walk_approval_requests(child)
