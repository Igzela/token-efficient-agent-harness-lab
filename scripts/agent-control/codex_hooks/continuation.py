"""WorkCard completion evaluation and autonomous continuation loop (H3).

Implements the Stop hook logic:
- Inspects workspace changes to verify WorkCard progress.
- If incomplete and continuation budget remains, intercepts stop and prompts continuation.
- If complete or budget exhausted, allows graceful termination.
"""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
from pathlib import Path
import subprocess
from typing import Any

from .protocol import HookInput, HookOutput, HookSpecificOutput
from .telemetry import HookTelemetry


@dataclass
class ContinuationDecision:
    """Outcome of the Stop evaluation."""

    allow_stop: bool
    continuation_prompt: str | None = None
    attempt: int = 0
    reason: str = ""


class ContinuationHandler:
    """Evaluates Stop hooks to prevent premature stopping on incomplete WorkCards."""

    def __init__(self, state_dir: Path | str | None = None, max_continuations: int | None = None):
        if state_dir is not None:
            self.state_dir = Path(state_dir)
        else:
            env_dir = os.environ.get("STEWARD_SESSION_STATE_DIR")
            self.state_dir = Path(env_dir) if env_dir else Path("/tmp/codex_hooks_state")
        self.state_dir.mkdir(parents=True, exist_ok=True)
        self.state_file = self.state_dir / "continuation_state.json"
        if max_continuations is not None:
            self.max_continuations = max_continuations
        else:
            self.max_continuations = int(os.environ.get("STEWARD_MAX_CONTINUATIONS", "2"))
        self.telemetry = HookTelemetry(self.state_dir)

    def _get_attempts(self) -> int:
        if self.state_file.is_file() and not self.state_file.is_symlink():
            try:
                data = json.loads(self.state_file.read_text(encoding="utf-8"))
                return int(data.get("continuation_attempts", 0))
            except Exception:
                pass
        return 0

    def _record_attempt(self, attempt: int) -> None:
        self.state_file.write_text(
            json.dumps({"continuation_attempts": attempt}, indent=2),
            encoding="utf-8",
        )

    def evaluate_stop(self, hook_input: HookInput) -> ContinuationDecision:
        """Evaluate if stopping is permitted or if continuation prompt is required."""
        worktree = Path(os.environ.get("STEWARD_WORKTREE", os.getcwd())).resolve()
        card_id = os.environ.get("STEWARD_WORKCARD_ID", "")
        worker_type = os.environ.get("STEWARD_WORKER_TYPE", "implement")

        # Review worker does not produce file changes; stopping is always permitted
        if worker_type == "review":
            return ContinuationDecision(allow_stop=True, reason="review_worker_completion")

        # Check git status for modifications
        has_changes = False
        try:
            proc = subprocess.run(
                ["git", "-C", str(worktree), "status", "--porcelain"],
                capture_output=True,
                text=True,
                timeout=5,
                check=False,
            )
            if proc.returncode == 0:
                # Filter out internal control plane artifacts such as hooks_state or .codex
                meaningful_changes = [
                    line for line in proc.stdout.splitlines()
                    if line.strip() and not any(
                        ignored in line for ignored in ("hooks_state", ".codex", "failure_reason.json")
                    )
                ]
                if meaningful_changes:
                    has_changes = True
        except Exception:
            pass

        # If changes exist, WorkCard produced edits -> allow stop
        if has_changes:
            return ContinuationDecision(allow_stop=True, reason="workcard_changes_present")

        # No changes produced yet! Check continuation budget
        current_attempts = self._get_attempts()
        if current_attempts < self.max_continuations:
            new_attempts = current_attempts + 1
            self._record_attempt(new_attempts)
            reason = f"no_workspace_changes_attempt_{new_attempts}_of_{self.max_continuations}"
            prompt = (
                f"WorkCard {card_id} is incomplete: no files have been modified in the workspace. "
                "Please implement the required changes according to the WorkCard objective "
                "and verify locally before stopping."
            )
            self.telemetry.record_stop_intercept(new_attempts, reason)
            return ContinuationDecision(
                allow_stop=False,
                continuation_prompt=prompt,
                attempt=new_attempts,
                reason=reason,
            )

        # Continuation budget exhausted -> allow stop
        return ContinuationDecision(
            allow_stop=True,
            reason=f"continuation_budget_exhausted_{current_attempts}_attempts",
        )

    def handle_stop(self, hook_input: HookInput) -> tuple[int, HookOutput, str | None]:
        """Process Stop event, returning exit_code, HookOutput, and optional stderr continuation message."""
        decision = self.evaluate_stop(hook_input)
        if decision.allow_stop:
            return 0, HookOutput(
                hookSpecificOutput=HookSpecificOutput(
                    stopReason=decision.reason,
                )
            ), None
        else:
            # Codex Stop hook: exit 2 with continuation prompt on stderr
            return 2, HookOutput(
                hookSpecificOutput=HookSpecificOutput(
                    stopReason=decision.reason,
                )
            ), decision.continuation_prompt
