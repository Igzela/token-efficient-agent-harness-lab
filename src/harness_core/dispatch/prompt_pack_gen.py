"""PromptPack schema and generator for manual execution bridge."""

from __future__ import annotations

import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

from .dispatch_decision import DispatchDecision

# ---------------------------------------------------------------------------
# Schema version
# ---------------------------------------------------------------------------

PROMPT_PACK_SCHEMA_VERSION = "prompt_pack.v1"

# ---------------------------------------------------------------------------
# Evaluation checklist defaults by risk level
# ---------------------------------------------------------------------------

_DEFAULT_CHECKLIST: tuple[str, ...] = (
    "schema_validity",
    "boundary_compliance",
    "output_present",
    "error_free",
)

_HIGH_RISK_CHECKLIST: tuple[str, ...] = (
    "schema_validity",
    "boundary_compliance",
    "output_present",
    "error_free",
    "human_review_required",
)


# ---------------------------------------------------------------------------
# Schema
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class PromptPack:
    prompt_pack_id: str
    dispatch_id: str
    recommended_model_tier: str
    recommended_profile_id: str | None
    system_prompt: str
    user_prompt: str
    context_pack_refs: tuple[str, ...]
    max_input_tokens: int
    max_output_tokens: int
    expected_output_schema: dict[str, Any] | None
    forbidden_outputs: tuple[str, ...]
    evaluation_checklist: tuple[str, ...]
    pasteback_instructions: str
    created_at: str
    schema_version: str = PROMPT_PACK_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "prompt_pack_id": self.prompt_pack_id,
            "dispatch_id": self.dispatch_id,
            "recommended_model_tier": self.recommended_model_tier,
            "recommended_profile_id": self.recommended_profile_id,
            "system_prompt": self.system_prompt,
            "user_prompt": self.user_prompt,
            "context_pack_refs": list(self.context_pack_refs),
            "max_input_tokens": self.max_input_tokens,
            "max_output_tokens": self.max_output_tokens,
            "expected_output_schema": self.expected_output_schema,
            "forbidden_outputs": list(self.forbidden_outputs),
            "evaluation_checklist": list(self.evaluation_checklist),
            "pasteback_instructions": self.pasteback_instructions,
            "created_at": self.created_at,
        }


# ---------------------------------------------------------------------------
# Generator
# ---------------------------------------------------------------------------


class PromptPackGenerator:
    """Generates PromptPacks from DispatchDecisions."""

    def generate(self, decision: DispatchDecision, raw_request: str) -> PromptPack:
        checklist = self._select_checklist(decision)
        system_prompt = self._build_system_prompt(decision)
        user_prompt = self._build_user_prompt(raw_request, decision)
        forbidden = self._forbidden_outputs(decision)
        instructions = self._pasteback_instructions(decision)

        return PromptPack(
            prompt_pack_id=f"pp-{uuid.uuid4().hex[:12]}",
            dispatch_id=decision.decision_id,
            recommended_model_tier=decision.selected_tier,
            recommended_profile_id=decision.selected_profile_id,
            system_prompt=system_prompt,
            user_prompt=user_prompt,
            context_pack_refs=(),
            max_input_tokens=decision.max_input_tokens,
            max_output_tokens=decision.max_output_tokens,
            expected_output_schema=None,
            forbidden_outputs=forbidden,
            evaluation_checklist=checklist,
            pasteback_instructions=instructions,
            created_at=datetime.now(timezone.utc).isoformat(),
        )

    def _select_checklist(self, decision: DispatchDecision) -> tuple[str, ...]:
        if decision.execution_policy.get("requires_human_review"):
            return _HIGH_RISK_CHECKLIST
        return _DEFAULT_CHECKLIST

    def _build_system_prompt(self, decision: DispatchDecision) -> str:
        tier = decision.selected_tier
        band = decision.expected_quality_band
        return (
            f"You are a {tier} model (quality band: {band}). "
            f"Complete the following task within the token budget of "
            f"{decision.max_input_tokens} input / {decision.max_output_tokens} output tokens."
        )

    def _build_user_prompt(self, raw_request: str, decision: DispatchDecision) -> str:
        return raw_request

    def _forbidden_outputs(self, decision: DispatchDecision) -> tuple[str, ...]:
        forbidden: list[str] = []
        for constraint in decision.hard_constraints:
            if constraint == "no_provider_call":
                forbidden.append("Do not make API calls to external providers.")
            elif constraint == "no_target_write":
                forbidden.append("Do not write to target repositories.")
            elif constraint == "requires_human_approval":
                forbidden.append("Do not execute without human approval.")
        return tuple(forbidden)

    def _pasteback_instructions(self, decision: DispatchDecision) -> str:
        return (
            f"Execute the task using model tier '{decision.selected_tier}' "
            f"with up to {decision.max_output_tokens} output tokens. "
            f"Then paste the complete output back for evaluation. "
            f"Include the model name and provider used, if known."
        )
