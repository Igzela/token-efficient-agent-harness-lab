"""WorkCard completion evaluation and autonomous continuation loop (H3).

Implements the Stop hook logic:
- Verifies declared WorkCard acceptance and verification evidence rather than
  raw git status mutations.
- If incomplete and continuation budget remains, blocks stop (top-level decision="block"
  + reason) and prompts continuation.
- If budget is exhausted or paused, exits with recorded incomplete status.
- If complete, allows graceful stop.
"""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
from pathlib import Path
import subprocess
from typing import Any

from .evidence import (
    build_evidence_record,
    evidence_binding_matches,
    porcelain_work_product_lines,
    read_allowed_paths,
    read_expected_evidence,
    read_focused_tests,
    read_negative_checks,
    workcard_acceptance_digest,
)
from .protocol import HookInput, HookOutput
from .telemetry import HookTelemetry


@dataclass
class ContinuationDecision:
    """Outcome of the Stop evaluation."""

    allow_stop: bool
    continuation_prompt: str | None = None
    attempt: int = 0
    reason: str = ""
    is_incomplete: bool = False


class ContinuationHandler:
    """Evaluates Stop hooks to verify WorkCard acceptance and prevent premature stopping."""

    def __init__(self, state_dir: Path | str | None = None, max_continuations: int | None = None):
        if state_dir is not None:
            self.state_dir = Path(state_dir)
        else:
            env_dir = os.environ.get("STEWARD_SESSION_STATE_DIR")
            self.state_dir = Path(env_dir) if env_dir else Path("/tmp/codex_hooks_state")
        self.state_dir.mkdir(parents=True, exist_ok=True)
        self.state_file = self.state_dir / "continuation_state.json"
        self.completion_file = self.state_dir / "completion_status.json"
        self.evidence_file = self.state_dir / "verification_evidence.json"
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

    def _record_completion_status(self, status: str, reason: str, attempts: int = 0) -> None:
        data = {
            "status": status,
            "reason": reason,
            "attempts": attempts,
            "card_id": os.environ.get("STEWARD_WORKCARD_ID", ""),
        }
        self.completion_file.write_text(json.dumps(data, indent=2), encoding="utf-8")

    def _verify_workspace_edits(self, worktree: Path, allowed_paths: list[str]) -> tuple[bool, str]:
        """Verify that files within allowed_paths were actually modified or created."""
        try:
            proc = subprocess.run(
                ["git", "-C", str(worktree), "status", "--porcelain"],
                capture_output=True,
                text=True,
                timeout=5,
                check=False,
            )
            if proc.returncode != 0:
                return False, "git_status_check_failed"
        except Exception as exc:
            return False, f"git_status_error: {exc}"

        lines = porcelain_work_product_lines(proc.stdout)
        if not lines:
            return False, "no_files_modified_in_workspace"

        # Check that at least one modified file matches allowed_paths and is not internal state
        in_scope_edits: list[str] = []
        for line in lines:
            # porcelain format: XY <path> or XY <orig> -> <new>
            parts = line.split(maxsplit=1)
            if len(parts) < 2:
                continue
            path_part = parts[1].split("->")[-1].strip().strip('"')

            # Check against allowed_paths
            for allow in allowed_paths:
                allow_clean = allow.strip().rstrip("/")
                if path_part == allow_clean or path_part.startswith(f"{allow_clean}/"):
                    in_scope_edits.append(path_part)
                    break

        if not in_scope_edits:
            return False, "no_modifications_found_within_allowed_scope"

        return True, ""

    def _verify_acceptance_evidence(
        self,
        worktree: Path,
        focused_tests: list[str],
        expected_evidence: list[str],
        negative_checks: list[str] | None = None,
        allowed_paths: list[str] | None = None,
    ) -> tuple[bool, str]:
        """Verify that declared verification evidence or focused tests are satisfied.

        Stored PASS evidence is accepted only when bound to the current
        WorkCard id, complete acceptance digest (focused_tests, negative_checks,
        expected_evidence descriptors, allowed_paths), command, and code/workspace state.
        Stale records from old WorkCards, moved code, or mismatched acceptance
        descriptors are rejected and the focused tests are re-executed for fresh evidence.

        Note: expected_evidence contains WorkCard acceptance descriptors
        (e.g. 'implementation head', 'focused-check receipt', 'independent
        review receipt'), NOT file paths. Hooks never interpret them with
        Path.exists() and never act as a secondary evaluator for Steward-owned
        gates (independent review, CI, PR merge).
        """
        card_id = os.environ.get("STEWARD_WORKCARD_ID", "").strip()

        # 1. Bound evidence receipt must match the current WorkCard acceptance contract
        if self.evidence_file.is_file() and not self.evidence_file.is_symlink():
            try:
                ev_data = json.loads(self.evidence_file.read_text(encoding="utf-8"))
            except Exception:
                ev_data = None
            if ev_data is not None:
                bound, reason = evidence_binding_matches(
                    ev_data,
                    workcard_id=card_id,
                    focused_tests=focused_tests,
                    negative_checks=negative_checks,
                    expected_evidence=expected_evidence,
                    allowed_paths=allowed_paths,
                    worktree=worktree,
                )
                if bound:
                    return True, ""
                # Stale/unbound evidence is ignored (never accepted); fall
                # through to re-execution so fresh evidence can be produced.

        # 2. If focused_tests declared, execute them or require verification receipt
        if focused_tests:
            # Attempt to run focused tests directly if no passing receipt was found
            for test_target in focused_tests:
                target_str = str(test_target).strip()
                if not target_str:
                    continue
                cmd: list[str]
                if target_str.endswith(".py"):
                    cmd = ["uv", "run", "--no-project", "pytest", target_str]
                else:
                    cmd = target_str.split()
                try:
                    res = subprocess.run(
                        cmd,
                        cwd=str(worktree),
                        capture_output=True,
                        text=True,
                        timeout=30,
                        check=False,
                    )
                    if res.returncode != 0:
                        return False, f"focused_test_failed: {target_str} (exit {res.returncode})"
                except Exception as exc:
                    return False, f"focused_test_execution_error: {exc}"

            # All focused tests passed with real exit-status semantics: record
            # bound evidence so the next Stop can accept it without re-running.
            record = build_evidence_record(
                workcard_id=card_id,
                focused_tests=focused_tests,
                negative_checks=negative_checks,
                expected_evidence=expected_evidence,
                allowed_paths=allowed_paths,
                command=" ".join(cmd) if focused_tests else "",
                success=True,
                worktree=worktree,
                receipt_id=0,
            )
            self.evidence_file.write_text(json.dumps(record, indent=2), encoding="utf-8")
            return True, ""

        # If no focused tests are declared, having in-scope edits is sufficient
        return True, ""

    def evaluate_stop(self, hook_input: HookInput) -> ContinuationDecision:
        """Evaluate if stopping is permitted or if continuation prompt is required."""
        worktree_raw = os.environ.get("STEWARD_WORKTREE", "")
        worktree = Path(worktree_raw).resolve() if worktree_raw else Path(os.getcwd()).resolve()
        card_id = os.environ.get("STEWARD_WORKCARD_ID", "").strip()
        worker_type = os.environ.get("STEWARD_WORKER_TYPE", "implement")

        # Review worker does not produce file changes; stopping is permitted
        if worker_type == "review":
            self._record_completion_status("completed", reason="review_worker_completion")
            return ContinuationDecision(allow_stop=True, reason="review_worker_completion")

        # Missing WorkCard context -> fail closed
        if not card_id:
            return ContinuationDecision(
                allow_stop=False,
                continuation_prompt="WorkCard ID context is missing from environment.",
                reason="missing_workcard_context",
            )

        allowed_paths = read_allowed_paths()
        focused_tests = read_focused_tests()
        expected_evidence = read_expected_evidence()
        negative_checks = read_negative_checks()

        # 1. Verify in-scope workspace edits
        edits_ok, edits_err = self._verify_workspace_edits(worktree, allowed_paths)
        if not edits_ok:
            return self._handle_incomplete(card_id, edits_err)

        # 2. Verify declared acceptance evidence and focused tests
        ev_ok, ev_err = self._verify_acceptance_evidence(
            worktree=worktree,
            focused_tests=focused_tests,
            expected_evidence=expected_evidence,
            negative_checks=negative_checks,
            allowed_paths=allowed_paths,
        )
        if not ev_ok:
            return self._handle_incomplete(card_id, ev_err)

        # WorkCard acceptance criteria verified!
        self._record_completion_status("completed", reason="acceptance_evidence_verified")
        return ContinuationDecision(allow_stop=True, reason="acceptance_evidence_verified")

    def _handle_incomplete(self, card_id: str, unfulfilled_reason: str) -> ContinuationDecision:
        """Handle incomplete acceptance criteria by blocking stop or exhausting budget."""
        current_attempts = self._get_attempts()
        if current_attempts < self.max_continuations:
            new_attempts = current_attempts + 1
            self._record_attempt(new_attempts)
            reason = f"acceptance_unfulfilled_attempt_{new_attempts}_of_{self.max_continuations}"
            prompt = (
                f"WorkCard {card_id} is incomplete ({unfulfilled_reason}). "
                "Please implement the required changes within allowed paths, run and pass "
                "the declared focused verification checks, and verify evidence before stopping."
            )
            self.telemetry.record_stop_intercept(new_attempts, unfulfilled_reason)
            return ContinuationDecision(
                allow_stop=False,
                continuation_prompt=prompt,
                attempt=new_attempts,
                reason=reason,
            )

        # Continuation budget exhausted -> exit with recorded incomplete status
        reason = f"continuation_budget_exhausted_{current_attempts}_attempts"
        self._record_completion_status("incomplete", reason=f"budget_exhausted: {unfulfilled_reason}", attempts=current_attempts)
        return ContinuationDecision(
            allow_stop=True,
            attempt=current_attempts,
            reason=reason,
            is_incomplete=True,
        )

    def handle_stop(self, hook_input: HookInput) -> tuple[int, HookOutput, str | None]:
        """Process Stop event, returning exit_code, HookOutput, and optional stderr message.

        Always exits 0 with a decision document: the Codex runtime only parses
        hook stdout on exit 0, so a block signaled via nonzero exit would be
        ignored and the session would stop fail-open.
        """
        decision = self.evaluate_stop(hook_input)
        if decision.allow_stop:
            return 0, HookOutput(
                continue_=True,
                stopReason=decision.reason,
            ), None
        else:
            # Stop is blocked: official schema top-level decision="block" + non-empty reason
            prompt = decision.continuation_prompt or "Stop blocked: WorkCard acceptance unfulfilled"
            output = HookOutput(
                continue_=True,
                decision="block",
                reason=prompt,
                stopReason=decision.reason,
            )
            return 0, output, prompt
