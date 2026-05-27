"""ManualUsageBridge: bridges PastebackSubmission → UsageLedgerRow."""

from __future__ import annotations

import uuid
from datetime import datetime, timezone
from typing import Any

from ..usage_ledger import UsageLedgerRow
from .manual_evaluator import ManualEvalResult
from .pasteback_parser import PastebackSubmission, ESTIMATED_CHARS_PER_TOKEN
from .prompt_pack_gen import PromptPack


# ---------------------------------------------------------------------------
# Bridge
# ---------------------------------------------------------------------------


class ManualUsageBridge:
    """Creates UsageLedgerRows from PastebackSubmissions."""

    def __init__(self, default_cost_per_1k: float = 0.002) -> None:
        self._default_cost_per_1k = default_cost_per_1k

    def bridge(
        self,
        submission: PastebackSubmission,
        eval_result: ManualEvalResult,
        prompt_pack: PromptPack | None = None,
        case_id: str = "manual_dispatch",
        cost_of_pass_group: str = "manual/unknown/unknown/unknown",
        model_profile_id: str = "unknown",
        context_pack_id: str = "none",
    ) -> UsageLedgerRow:
        if submission.claimed_input_tokens:
            input_tokens = submission.claimed_input_tokens
        elif prompt_pack:
            input_tokens = self._estimate_tokens(prompt_pack.system_prompt + prompt_pack.user_prompt)
        else:
            input_tokens = self._estimate_tokens(submission.raw_output)

        output_tokens = submission.claimed_output_tokens or self._estimate_tokens(submission.raw_output)
        cost = submission.claimed_cost or self._estimate_cost(input_tokens, output_tokens)

        passed = eval_result.status == "pass"

        return UsageLedgerRow(
            run_id=f"run-{uuid.uuid4().hex[:12]}",
            case_id=case_id,
            input_tokens=input_tokens,
            output_tokens=output_tokens,
            cached_tokens=0,
            request_count=1,
            tool_call_count=0,
            retry_count=0,
            wall_clock_ms=0,
            estimated_cost=cost,
            pass_=passed,
            cost_of_pass_group=cost_of_pass_group,
            model_profile_id=model_profile_id,
            context_pack_id=context_pack_id,
        )

    def _estimate_tokens(self, text: str) -> int:
        return max(1, len(text) // ESTIMATED_CHARS_PER_TOKEN)

    def _estimate_cost(self, input_tokens: int, output_tokens: int) -> float:
        return round((input_tokens + output_tokens) / 1000 * self._default_cost_per_1k, 6)
