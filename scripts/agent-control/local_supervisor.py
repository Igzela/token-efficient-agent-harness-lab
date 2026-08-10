"""Bounded process supervision for the repository-owned local loop."""

from __future__ import annotations

import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
from typing import Any, Callable
import uuid

import local_loop
import local_run_once
import plan_lane
import state_manager


class LocalSupervisor:
    """Stateless K=2 launcher built on the same run-once entrypoint."""

    def __init__(
        self,
        controller: local_loop.LoopController,
        *,
        repository: str,
        repo_path: Path,
        max_active: int = state_manager.MAX_ACTIVE,
        task_timeout_seconds: int = 3600,
        sleeper: Callable[[float], None] = time.sleep,
    ) -> None:
        if max_active < 1 or max_active > state_manager.MAX_ACTIVE:
            raise ValueError(f"max_active must be between 1 and {state_manager.MAX_ACTIVE}")
        if task_timeout_seconds < 1 or task_timeout_seconds > 7200:
            raise ValueError("task_timeout_seconds is outside the bounded range")
        if not local_loop.REPOSITORY.fullmatch(repository):
            raise ValueError("repository must be owner/name")
        self.controller = controller
        self.repository = repository
        self.repo_path = Path(repo_path).expanduser().resolve()
        self.max_active = max_active
        self.task_timeout_seconds = task_timeout_seconds
        self.sleeper = sleeper

    def _terminate(self, process: subprocess.Popen[str]) -> None:
        local_run_once.terminate_task_process_group(process)

    def _unknown_receipt(self, issue: int, attempt: str, reason: str) -> dict[str, Any]:
        return {
            "kind": "repo-agent-local-run-once.v1",
            "status": "outcome_unknown",
            "issue_number": issue,
            "attempt_id": attempt,
            "details": {"reason": reason},
        }

    def _reconcile_unknown(self, issue: int | None, attempt: str, plan_id: str | None = None) -> dict[str, Any]:
        """Ask the existing controller to terminalize an unknown attempt."""

        github = getattr(self.controller, "github", None)
        if github is None:
            return self._unknown_receipt(issue, attempt, "timeout_reconciliation_unavailable")
        if plan_id is not None:
            subject = {
                "kind": "repo-agent-local-run-once.v1",
                "issue_number": 0,
                "subject_kind": "plan-packet",
                "subject_id": plan_id,
                "attempt_id": attempt,
            }
            try:
                ledger_issue = github.plan_ledger_issue()
                comments = state_manager.get_issue_comments(ledger_issue, self.repository)
            except (AttributeError, local_loop.LoopUnavailable, state_manager.StateUnavailableError):
                return {**subject, "status": "outcome_unknown", "details": {
                    "reason": "plan_reconciliation_unavailable",
                }}
            matches: list[dict[str, Any]] = []
            seen_dispatch_ids: set[str] = set()
            for comment in comments:
                if (comment.get("author") or {}).get("login") not in state_manager.TRUSTED_STATE_AUTHORS:
                    continue
                body = comment.get("body", "")
                if not isinstance(body, str) or "agent-orchestrator-dispatch-state" not in body:
                    continue
                try:
                    state = json.loads(body)
                except (TypeError, json.JSONDecodeError):
                    return {**subject, "status": "outcome_unknown", "details": {
                        "reason": "plan_claim_state_malformed",
                    }}
                if not isinstance(state, dict) or state.get("kind") != "agent-orchestrator-dispatch-state":
                    continue
                if state.get("version") != 1 or state.get("issue_number") != ledger_issue:
                    return {**subject, "status": "outcome_unknown", "details": {
                        "reason": "plan_claim_state_invalid",
                    }}
                if state.get("action") != "plan-run":
                    continue
                details = state.get("details")
                dispatch_id = state.get("dispatch_id")
                if not isinstance(dispatch_id, str) or not state_manager.PLAN_DISPATCH_ID_PATTERN.fullmatch(dispatch_id):
                    return {**subject, "status": "outcome_unknown", "details": {
                        "reason": "plan_claim_binding_invalid",
                    }}
                if dispatch_id in seen_dispatch_ids:
                    continue
                seen_dispatch_ids.add(dispatch_id)
                if (
                    isinstance(details, dict)
                    and details.get("subject_kind") == "plan-packet"
                    and details.get("subject_id") == plan_id
                    and details.get("attempt_id") == attempt
                    and state.get("status") in {"claimed", "dispatched", "failed_unknown_output", "closed_out"}
                ):
                    matches.append(state)
            if len(matches) != 1:
                return {**subject, "status": "outcome_unknown", "details": {
                    "reason": "plan_claim_state_ambiguous" if len(matches) > 1 else "plan_claim_not_found",
                }}
            claim = matches[0]
            details = claim["details"]
            source_main_sha = details.get("source_main_sha")
            claim_nonce = details.get("claim_nonce")
            dispatch_id = claim.get("dispatch_id")
            if (
                not isinstance(source_main_sha, str)
                or not local_loop.HEX40.fullmatch(source_main_sha)
                or not isinstance(claim_nonce, str)
                or not state_manager.CLAIM_NONCE_PATTERN.fullmatch(claim_nonce)
                or dispatch_id != f"plan-run:{plan_id}:{source_main_sha}:{attempt}"
            ):
                return {**subject, "status": "outcome_unknown", "details": {
                    "reason": "plan_claim_binding_invalid",
                }}
            if claim.get("status") == "failed_unknown_output":
                return {**subject, "status": "failed_unknown_output", "details": {
                    "reason": "timeout_reconciled",
                }}
            if claim.get("status") == "closed_out":
                return {**subject, "status": "closed_out", "details": {
                    "reason": "closed_out",
                }}
            try:
                github.dispatch_controller(
                    "block-plan",
                    {
                        "packet_id": plan_id,
                        "attempt_id": attempt,
                        "source_main_sha": source_main_sha,
                        "claim_nonce": claim_nonce,
                    },
                )
            except local_loop.LoopUnavailable:
                return {**subject, "status": "outcome_unknown", "details": {
                    "reason": "timeout_reconciliation_outcome_unknown",
                }}
            deadline = time.monotonic() + min(30, self.task_timeout_seconds)
            while True:
                try:
                    state = state_manager.read_dispatch_state(
                        ledger_issue, dispatch_id, self.repository
                    )
                except state_manager.StateUnavailableError:
                    state = None
                if isinstance(state, dict) and state.get("status") == "failed_unknown_output":
                    return {**subject, "status": "failed_unknown_output", "details": {
                        "reason": "timeout_reconciled",
                    }}
                if time.monotonic() >= deadline:
                    return {**subject, "status": "outcome_unknown", "details": {
                        "reason": "timeout_reconciliation_unproven",
                    }}
                self.sleeper(0.05)
        if issue is None:
            return self._unknown_receipt(0, attempt, "subject_unavailable")
        token = local_loop.local_client_token(self.repository, issue, attempt)
        dispatch_id = f"local-run:{issue}:{attempt}"
        try:
            claim = state_manager.read_dispatch_state(issue, dispatch_id, self.repository)
        except state_manager.StateUnavailableError:
            claim = None
        details = claim.get("details") if isinstance(claim, dict) else None
        claim_nonce = details.get("claim_nonce") if isinstance(details, dict) else None
        if not isinstance(claim_nonce, str):
            return self._unknown_receipt(issue, attempt, "timeout_claim_nonce_unavailable")
        try:
            github.dispatch_controller(
                "block-local",
                {
                    "issue": issue,
                    "attempt_id": attempt,
                    "client_token": token,
                    "reason_code": "local_unknown_output",
                    "claim_nonce": claim_nonce,
                },
            )
        except local_loop.LoopUnavailable:
            return self._unknown_receipt(issue, attempt, "timeout_reconciliation_outcome_unknown")
        deadline = time.monotonic() + min(30, self.task_timeout_seconds)
        while True:
            try:
                state = state_manager.read_dispatch_state(issue, dispatch_id, self.repository)
            except state_manager.StateUnavailableError:
                state = None
            if isinstance(state, dict) and state.get("status") == "failed_unknown_output":
                return {
                    "kind": "repo-agent-local-run-once.v1",
                    "status": "failed_unknown_output",
                    "issue_number": issue,
                    "attempt_id": attempt,
                    "details": {"reason": "timeout_reconciled"},
                }
            if time.monotonic() >= deadline:
                return self._unknown_receipt(issue, attempt, "timeout_reconciliation_unproven")
            self.sleeper(0.05)

    @staticmethod
    def _child_receipt(
        process: subprocess.Popen[str], issue: int | None, attempt: str,
        *, subject_kind: str = "issue", subject_id: Any = None,
    ) -> dict[str, Any]:
        """Accept exactly one schema-bound receipt with a consistent exit code."""

        try:
            stdout, _stderr = process.communicate(timeout=2)
        except subprocess.TimeoutExpired:
            if process.stdout is not None:
                process.stdout.close()
            if process.stderr is not None:
                process.stderr.close()
            return {
                "kind": "repo-agent-local-run-once.v1",
                "status": "outcome_unknown",
                "issue_number": issue,
                "attempt_id": attempt,
                "details": {"reason": "child_output_timeout"},
            }
        if len(stdout.encode("utf-8", errors="replace")) > 64 * 1024:
            return {
                "kind": "repo-agent-local-run-once.v1",
                "status": "outcome_unknown",
                "issue_number": issue,
                "attempt_id": attempt,
                "details": {"reason": "child_receipt_too_large"},
            }
        lines = [line for line in stdout.splitlines() if line.strip()]
        if len(lines) != 1:
            return {
                "kind": "repo-agent-local-run-once.v1",
                "status": "outcome_unknown",
                "issue_number": issue,
                "attempt_id": attempt,
                "details": {"reason": "child_receipt_count_invalid"},
            }
        try:
            receipt = json.loads(lines[0])
        except json.JSONDecodeError:
            receipt = None
        valid_statuses = {
            "handed_off", "failed", "failed_unknown_output", "closed_out", "outcome_unknown",
            "rejected", "control_stopped", "stale_checkout", "claim_unavailable",
            "claim_rejected", "in_flight", "terminal", "identity_rejected", "unavailable",
        }
        subject_binding_valid = isinstance(receipt, dict) and (
            receipt.get("issue_number") == issue
            if subject_kind == "issue"
            else receipt.get("subject_kind") == "plan-packet"
            and receipt.get("subject_id") == subject_id
        )
        if (
            not isinstance(receipt, dict)
            or receipt.get("kind") != "repo-agent-local-run-once.v1"
            or not subject_binding_valid
            or receipt.get("attempt_id") != attempt
            or receipt.get("status") not in valid_statuses
            or (process.returncode == 0) != (receipt.get("status") == "handed_off")
        ):
            return {
                "kind": "repo-agent-local-run-once.v1",
                "status": "outcome_unknown",
                "issue_number": issue,
                "attempt_id": attempt,
                "details": {"reason": "child_receipt_binding_invalid"},
            }
        return receipt

    def run_batch(self) -> dict[str, Any]:
        decision = self.controller.poll()
        if decision.get("status") != "ready":
            return {"kind": "repo-agent-supervisor.v1", "decision": decision, "results": []}
        selected = decision.get("selected")
        if not isinstance(selected, list) or not selected or len(selected) > self.max_active:
            return {
                "kind": "repo-agent-supervisor.v1",
                "status": "unavailable",
                "reason": "poll_capacity_contract_violation",
                "results": [],
            }
        validated: list[tuple[dict[str, Any], str]] = []
        seen_issues: set[int] = set()
        seen_plans: set[str] = set()
        for candidate in selected:
            if not isinstance(candidate, dict):
                return {
                    "kind": "repo-agent-supervisor.v1",
                    "status": "unavailable",
                    "reason": "poll_candidate_invalid",
                    "results": [],
                }
            kind = candidate.get("candidate_kind", "issue")
            if kind == "issue":
                issue = candidate.get("issue_number")
                if type(issue) is not int or issue <= 0 or issue in seen_issues:
                    return {
                        "kind": "repo-agent-supervisor.v1",
                        "status": "unavailable",
                        "reason": "poll_candidate_invalid",
                        "results": [],
                    }
                seen_issues.add(issue)
            elif kind == "plan":
                subject = candidate.get("subject_id")
                if not isinstance(subject, str) or not plan_lane.PACKET_ID.fullmatch(subject) or subject in seen_plans:
                    return {
                        "kind": "repo-agent-supervisor.v1",
                        "status": "unavailable",
                        "reason": "poll_candidate_invalid",
                        "results": [],
                    }
                seen_plans.add(subject)
            else:
                return {
                    "kind": "repo-agent-supervisor.v1",
                    "status": "unavailable",
                    "reason": "poll_candidate_invalid",
                    "results": [],
                }
            validated.append((candidate, str(uuid.uuid4())))

        children: list[dict[str, Any]] = []
        spawn_failures: list[dict[str, Any]] = []
        script = Path(__file__).resolve().with_name("loopctl.py")
        for candidate, attempt in validated:
            try:
                kind = candidate.get("candidate_kind", "issue")
                subject_args = (
                    ["--issue", str(candidate["issue_number"])]
                    if kind == "issue"
                    else ["--plan-id", candidate["subject_id"]]
                )
                process = subprocess.Popen(
                    [
                        sys.executable,
                        str(script),
                        "run-once",
                        "--repo",
                        self.repository,
                        "--repo-path",
                        str(self.repo_path),
                        *subject_args,
                        "--attempt-id",
                        attempt,
                    ],
                    cwd=self.repo_path,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                    start_new_session=True,
                )
            except (OSError, ValueError):
                spawn_failures.append({
                    "kind": "repo-agent-local-run-once.v1",
                    "issue_number": candidate.get("issue_number"),
                    "subject_kind": "plan-packet" if candidate.get("candidate_kind") == "plan" else "issue",
                    "subject_id": candidate.get("subject_id", candidate.get("issue_number")),
                    "attempt_id": attempt,
                    "status": "spawn_failed",
                })
                continue
            children.append({
                "process": process,
                "candidate": candidate,
                "attempt_id": attempt,
                "started": time.monotonic(),
            })

        results: list[dict[str, Any]] = list(spawn_failures)
        while children:
            remaining: list[dict[str, Any]] = []
            for child in children:
                process = child["process"]
                elapsed = time.monotonic() - child["started"]
                if process.poll() is None and elapsed > self.task_timeout_seconds:
                    self._terminate(process)
                    results.append(self._reconcile_unknown(
                        child["candidate"].get("issue_number"),
                        child["attempt_id"],
                        child["candidate"].get("subject_id")
                        if child["candidate"].get("candidate_kind") == "plan" else None,
                    ))
                    continue
                if process.poll() is None:
                    remaining.append(child)
                    continue
                receipt = self._child_receipt(
                    process,
                    child["candidate"].get("issue_number"),
                    child["attempt_id"],
                    subject_kind=(
                        "plan-packet" if child["candidate"].get("candidate_kind") == "plan" else "issue"
                    ),
                    subject_id=child["candidate"].get(
                        "subject_id", child["candidate"].get("issue_number")
                    ),
                )
                if receipt.get("status") == "outcome_unknown":
                    receipt = self._reconcile_unknown(
                        child["candidate"].get("issue_number"),
                        child["attempt_id"],
                        child["candidate"].get("subject_id")
                        if child["candidate"].get("candidate_kind") == "plan" else None,
                    )
                results.append(receipt)
            children = remaining
            if children:
                self.sleeper(0.05)
        return {
            "kind": "repo-agent-supervisor.v1",
            "status": "completed",
            "selected_issue_numbers": [
                candidate["issue_number"] for candidate, _attempt in validated
                if candidate.get("candidate_kind", "issue") == "issue"
            ],
            "selected_plan_subject_ids": [
                candidate["subject_id"] for candidate, _attempt in validated
                if candidate.get("candidate_kind") == "plan"
            ],
            "results": results,
        }
