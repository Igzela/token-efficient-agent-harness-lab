"""Single-task execution and recovery for the repository-owned local loop."""

from __future__ import annotations

import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import uuid
from typing import Any, Callable

import artifact_contract
import ci_verifier
import dispatcher
import local_loop
import local_verification
import plan_lane
import plan_lifecycle
import pr_binding
import prompt_builder
import route_driver
import state_manager
import worktree_manager


def _canonical_attempt_id(value: object) -> str | None:
    if not isinstance(value, str):
        return None
    try:
        parsed = uuid.UUID(value)
    except ValueError:
        return None
    return value if value == str(parsed) else None


def _pid_exists(pid: int) -> bool:
    if pid <= 0:
        return False
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def _process_children_map() -> dict[int, list[int]]:
    """Build parent → children from /proc without spawning helpers."""

    children: dict[int, list[int]] = {}
    try:
        entries = os.listdir("/proc")
    except OSError:
        return children
    for entry in entries:
        if not entry.isdigit():
            continue
        child_pid = int(entry)
        try:
            with open(f"/proc/{entry}/stat", "r", encoding="utf-8") as handle:
                payload = handle.read()
        except OSError:
            continue
        # comm may contain spaces/parentheses; ppid is the field after the
        # closing parenthesis of the command name.
        close = payload.rfind(")")
        if close < 0:
            continue
        fields = payload[close + 1 :].split()
        if len(fields) < 2:
            continue
        try:
            parent_pid = int(fields[1])
        except ValueError:
            continue
        children.setdefault(parent_pid, []).append(child_pid)
    return children


def _process_descendants(root_pid: int) -> list[int]:
    """Return every descendant of ``root_pid`` in deepest-first order."""

    children = _process_children_map()
    ordered: list[int] = []
    stack = list(children.get(root_pid, ()))
    seen: set[int] = set()
    while stack:
        pid = stack.pop()
        if pid in seen or pid == root_pid:
            continue
        seen.add(pid)
        grandchildren = children.get(pid, ())
        if grandchildren:
            stack.extend(grandchildren)
        ordered.append(pid)
    ordered.reverse()
    return ordered


def _signal_pids(pids: list[int], sig: int) -> None:
    for pid in pids:
        try:
            os.kill(pid, sig)
        except ProcessLookupError:
            continue
        except PermissionError:
            continue


def _terminate_process_tree(
    root_pid: int, *, term_timeout: float = 5, kill_timeout: float = 5
) -> None:
    """Bounded TERM then KILL for one process and every live descendant.

    Targets are selected by parent linkage, never by the caller's process
    group, so a timeout cannot signal the receipt owner that is waiting on
    the child.  PIDs collected before reparenting remain eligible for KILL
    even if they later move under init.
    """

    if root_pid <= 0:
        return
    descendants = _process_descendants(root_pid)
    targets = [pid for pid in descendants + [root_pid] if _pid_exists(pid)]
    if not targets:
        return
    _signal_pids(targets, signal.SIGTERM)
    deadline = time.monotonic() + max(0.05, term_timeout)
    while time.monotonic() < deadline:
        alive = [pid for pid in targets if _pid_exists(pid)]
        if not alive:
            return
        time.sleep(0.05)
    # A late fork during TERM must not escape the kill window.
    extra = _process_descendants(root_pid)
    kill_targets = []
    for pid in extra + targets:
        if pid not in kill_targets and _pid_exists(pid):
            kill_targets.append(pid)
    if not kill_targets:
        return
    _signal_pids(kill_targets, signal.SIGKILL)
    deadline = time.monotonic() + max(0.05, kill_timeout)
    while time.monotonic() < deadline:
        if not any(_pid_exists(pid) for pid in kill_targets):
            return
        time.sleep(0.05)


# Environment keys permitted into repository-owned worker children (Codex
# wrapper and focused checks).  Everything else — including GH_TOKEN,
# GITHUB_TOKEN, provider API keys, and cloud credentials — is dropped at
# process start so credentials never enter the model or check child.
_CHILD_ENV_ALLOWLIST = frozenset(
    {
        "HOME",
        "USER",
        "LOGNAME",
        "PATH",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "TMPDIR",
        "TMP",
        "TEMP",
        "TERM",
        "SHELL",
        "CODEX_HOME",
        "AGENT_CODEX_TIMEOUT_SECONDS",
        "PWD",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        # Operator-local network egress (e.g. clash/socks on loopback).  Not
        # credentials: without these, ChatGPT-backed Codex cannot reach the
        # provider from a fail-closed env -i child on proxy-only hosts.
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "REQUESTS_CA_BUNDLE",
        "CURL_CA_BUNDLE",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
    }
)


def child_env(base: dict[str, str] | None = None) -> dict[str, str]:
    """Return a fail-closed environment for untrusted or semi-trusted children."""

    source = base if base is not None else os.environ
    env: dict[str, str] = {}
    for key in _CHILD_ENV_ALLOWLIST:
        value = source.get(key)
        if isinstance(value, str) and value:
            env[key] = value
    # Never forward credential-shaped names even if allowlisted later by mistake.
    for key in list(env):
        upper = key.upper()
        if any(
            token in upper
            for token in (
                "TOKEN",
                "SECRET",
                "PASSWORD",
                "API_KEY",
                "APIKEY",
                "CREDENTIAL",
                "AUTH",
            )
        ):
            env.pop(key, None)
    if "PATH" not in env:
        env["PATH"] = "/usr/bin:/bin"
    if "HOME" not in env:
        env["HOME"] = str(Path.home())
    if "LANG" not in env:
        env["LANG"] = "C"
    if "LC_ALL" not in env:
        env["LC_ALL"] = "C"
    if "TERM" not in env:
        env["TERM"] = "dumb"
    return env


def _bounded_process(
    command: list[str],
    *,
    cwd: Path | None = None,
    timeout_seconds: int = 1800,
    env: dict[str, str] | None = None,
) -> tuple[int, str, str]:
    """Run one child in an isolated session with tree-scoped cancellation.

    The child is started with ``start_new_session=True`` so its process group
    is never the run-once/receipt owner.  Timeouts terminate only the child
    PID tree; the caller survives to emit a truthful non-success receipt.
    Credentials are never inherited: ``env`` defaults to ``child_env()``.
    """

    child_environment = child_env(env) if env is not None else child_env()
    try:
        process = subprocess.Popen(
            command,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            start_new_session=True,
            env=child_environment,
        )
    except (OSError, ValueError) as exc:
        raise local_loop.LoopUnavailable("local command could not start") from exc
    try:
        stdout, stderr = process.communicate(timeout=timeout_seconds)
        return process.returncode, stdout[-4000:], stderr[-4000:]
    except subprocess.TimeoutExpired:
        _terminate_process_tree(
            process.pid,
            term_timeout=min(10.0, max(1.0, timeout_seconds / 10)),
            kill_timeout=min(10.0, max(1.0, timeout_seconds / 10)),
        )
        try:
            stdout, stderr = process.communicate(timeout=2)
        except subprocess.TimeoutExpired:
            if process.stdout is not None:
                process.stdout.close()
            if process.stderr is not None:
                process.stderr.close()
            return 124, "", ""
        return 124, stdout[-4000:], stderr[-4000:]


def ensure_task_process_group() -> None:
    """Create a private task session for standalone ``loopctl run-once``."""

    if os.getpid() == os.getsid(0):
        return
    try:
        os.setsid()
    except OSError:
        # A process that is already a group leader cannot call setsid.  Do
        # not ever join or terminate a caller-owned parent group.
        return


def terminate_task_process_group(
    process: subprocess.Popen[str], *, term_timeout: float = 5, kill_timeout: float = 5
) -> None:
    """Terminate one task tree (leader plus descendants), never the caller."""

    try:
        root_pid = process.pid
    except (AttributeError, TypeError):
        return
    if type(root_pid) is not int or root_pid <= 0:
        return
    _terminate_process_tree(root_pid, term_timeout=term_timeout, kill_timeout=kill_timeout)
    try:
        process.wait(timeout=0.2)
    except subprocess.TimeoutExpired:
        # Bounded: a stuck zombie/pipe owner must not hang the supervisor.
        return


class LocalRunOnce:
    """Execute one claimed local implementation through existing owners."""

    def __init__(
        self,
        github: local_loop.GitHubReader | None = None,
        git: local_loop.GitReader | None = None,
        *,
        repository: str,
        repo_path: Path,
        claim_timeout_seconds: int = 120,
        command_timeout_seconds: int = 1800,
        poll_interval_seconds: float = 1.0,
        lifecycle_timeout_seconds: int = 3600,
        sleeper: Callable[[float], None] = time.sleep,
        promotion_output_provider: Callable[[str, Path], str | None] | None = None,
    ) -> None:
        if not local_loop.REPOSITORY.fullmatch(repository):
            raise ValueError("repository must be owner/name")
        if claim_timeout_seconds < 0 or claim_timeout_seconds > 900:
            raise ValueError("claim_timeout_seconds is outside the bounded range")
        if command_timeout_seconds < 1 or command_timeout_seconds > 3600:
            raise ValueError("command_timeout_seconds is outside the bounded range")
        if poll_interval_seconds < 0 or poll_interval_seconds > 30:
            raise ValueError("poll_interval_seconds is outside the bounded range")
        if lifecycle_timeout_seconds < 0 or lifecycle_timeout_seconds > 86400:
            raise ValueError("lifecycle_timeout_seconds is outside the bounded range")
        self.github = github or local_loop.GitHubAdapter(repository)
        self.git = git or local_loop.GitAdapter()
        self.repository = repository
        self.repo_path = Path(repo_path).expanduser().resolve()
        self.claim_timeout_seconds = claim_timeout_seconds
        self.command_timeout_seconds = command_timeout_seconds
        self.poll_interval_seconds = poll_interval_seconds
        self.lifecycle_timeout_seconds = lifecycle_timeout_seconds
        self.sleeper = sleeper
        # Tests may supply a bounded read-only worker result.  Production uses
        # the existing read-only Codex wrapper below; neither path grants a
        # child GitHub, merge, Provider-effect, or T3 capability.
        self.promotion_output_provider = promotion_output_provider

    def _result(self, status: str, issue: int, attempt: str, **details: Any):
        return local_loop.LocalRunOnceResult(status, issue, attempt, details)

    def _plan_result(self, status: str, packet_id: str, attempt: str, **details: Any):
        return self._result(
            status,
            0,
            attempt,
            subject_kind="plan-packet",
            subject_id=packet_id,
            **details,
        )

    def _client_token(self, issue: int, attempt: str) -> str:
        return local_loop.local_client_token(self.repository, issue, attempt)

    def _dispatch_id(self, issue: int, attempt: str) -> str:
        return f"local-run:{issue}:{attempt}"

    def _wait_for_claim(self, issue: int, dispatch_id: str) -> dict[str, Any] | None:
        deadline = time.monotonic() + self.claim_timeout_seconds
        while True:
            try:
                state = state_manager.read_dispatch_state(issue, dispatch_id, self.repository)
            except state_manager.StateUnavailableError:
                state = None
            if isinstance(state, dict):
                status = state.get("status")
                if status == "dispatched":
                    details = state.get("details")
                    return details if isinstance(details, dict) else None
                if status in {"failed", "rejected", "outcome_unknown", "failed_unknown_output"}:
                    return None
            if time.monotonic() >= deadline:
                return None
            self.sleeper(self.poll_interval_seconds)

    def _release(
        self, issue: int, attempt: str, token: str, claim_nonce: str, reason: str
    ) -> None:
        try:
            self.github.dispatch_controller(
                "release-local",
                {
                    "issue": issue,
                    "attempt_id": attempt,
                    "client_token": token,
                    "reason_code": reason,
                    "claim_nonce": claim_nonce,
                },
            )
        except local_loop.LoopUnavailable:
            return

    def _wait_for_handoff(
        self, issue: int, attempt: str, claim_nonce: str, pr_number: int, head_sha: str
    ) -> tuple[bool, str]:
        """Require worker, exact CI, and monitor receipts before success."""

        deadline = time.monotonic() + self.claim_timeout_seconds
        while True:
            try:
                worker = state_manager.read_worker_state(issue, self.repository)
                acquisition = state_manager.read_ci_acquisition(
                    issue, pr_number, head_sha, self.repository
                )
                run_id = acquisition.get("workflow_run_id") if isinstance(acquisition, dict) else None
                receipt = (
                    state_manager.read_dispatch_state(
                        issue, f"ci-monitor:{pr_number}:{head_sha}:{run_id}", self.repository
                    )
                    if type(run_id) is int and run_id > 0
                    else None
                )
            except state_manager.StateUnavailableError:
                worker = acquisition = receipt = None
                run_id = None
            worker_extra = worker.get("extra") if isinstance(worker, dict) else None
            if (
                isinstance(worker, dict)
                and worker.get("kind") == "agent-orchestrator-state"
                and worker.get("worker_type") == "local-run"
                and worker.get("pr_number") == pr_number
                and worker.get("head_sha") == head_sha
                and isinstance(worker_extra, dict)
                and worker_extra.get("attempt_id") == attempt
                and worker_extra.get("claim_nonce") == claim_nonce
                and isinstance(acquisition, dict)
                and acquisition.get("status") == "bound"
                and acquisition.get("workflow_run_id") == run_id
                and isinstance(receipt, dict)
                and receipt.get("kind") == "agent-orchestrator-dispatch-state"
                and receipt.get("action") == "ci-monitor"
                and receipt.get("status") == "dispatched"
            ):
                return True, "handoff_proven"
            if time.monotonic() >= deadline:
                return False, "handoff_state_unproven"
            self.sleeper(self.poll_interval_seconds)

    def _request_handoff(
        self, issue: int, attempt: str, token: str, claim_nonce: str, pr_number: int, head_sha: str
    ) -> tuple[bool, str]:
        try:
            self.github.dispatch_controller(
                "handoff-local",
                {
                    "issue": issue,
                    "attempt_id": attempt,
                    "client_token": token,
                    "head_sha": head_sha,
                    "claim_nonce": claim_nonce,
                },
            )
        except local_loop.LoopUnavailable:
            return False, "handoff_dispatch_outcome_unknown"
        return self._wait_for_handoff(issue, attempt, claim_nonce, pr_number, head_sha)

    def _unknown_output(
        self, issue: int, attempt: str, token: str, reason: str
    ) -> local_loop.LocalRunOnceResult:
        """Block an unprovable external effect; never release it as retryable."""

        try:
            claim_nonce = self._claim_nonce(issue, attempt)
        except local_loop.LoopUnavailable:
            return self._result("outcome_unknown", issue, attempt, reason=reason)
        try:
            self.github.dispatch_controller(
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
            return self._result("outcome_unknown", issue, attempt, reason=reason)
        deadline = time.monotonic() + min(self.claim_timeout_seconds, 30)
        dispatch_id = self._dispatch_id(issue, attempt)
        while True:
            try:
                state = state_manager.read_dispatch_state(issue, dispatch_id, self.repository)
            except state_manager.StateUnavailableError:
                state = None
            if isinstance(state, dict) and state.get("status") == "failed_unknown_output":
                return self._result("failed_unknown_output", issue, attempt, reason=reason)
            if time.monotonic() >= deadline:
                return self._result("outcome_unknown", issue, attempt, reason=reason)
            self.sleeper(self.poll_interval_seconds)

    def _claim_nonce(self, issue: int, attempt: str) -> str:
        """Read the exact trusted generation nonce for controller mutations."""

        try:
            state = state_manager.read_dispatch_state(
                issue, self._dispatch_id(issue, attempt), self.repository
            )
        except state_manager.StateUnavailableError as exc:
            raise local_loop.LoopUnavailable("claim state is unavailable") from exc
        details = state.get("details") if isinstance(state, dict) else None
        nonce = details.get("claim_nonce") if isinstance(details, dict) else None
        if not isinstance(nonce, str) or state_manager.CLAIM_NONCE_PATTERN.fullmatch(nonce) is None:
            raise local_loop.LoopUnavailable("claim nonce is unavailable")
        return nonce

    def _reconcile_unproven_claim(
        self, issue: int, attempt: str, token: str, dispatch_id: str
    ) -> None:
        """Release a same-attempt claim left without a proven dispatch handoff.

        Called when claim wait fails.  Only the exact attempt/token may release;
        mismatched or absent state is ignored so capacity owned by another
        generation is never demoted.
        """

        try:
            state = state_manager.read_dispatch_state(issue, dispatch_id, self.repository)
        except state_manager.StateUnavailableError:
            return
        if not isinstance(state, dict):
            return
        status = state.get("status")
        details = state.get("details") if isinstance(state.get("details"), dict) else None
        if status not in {"claimed", "dispatched"} or not isinstance(details, dict):
            return
        valid, _reason = state_manager.local_claim_binding_valid(
            issue, details, attempt, token, require_lease_live=False
        )
        if not valid:
            return
        nonce = details.get("claim_nonce")
        if not isinstance(nonce, str) or state_manager.CLAIM_NONCE_PATTERN.fullmatch(nonce) is None:
            return
        self._release(issue, attempt, token, nonce, "local_environment_failure")

    def _live_plan(self, packet_id: str) -> tuple[plan_lane.PlanCandidate, int]:
        if not plan_lane.PACKET_ID.fullmatch(packet_id):
            raise local_loop.LoopUnavailable("plan packet id is invalid")
        metadata = self.github.repository_metadata()
        branch = metadata.get("default_branch")
        if not isinstance(branch, str) or not local_loop.BRANCH.fullmatch(branch):
            raise local_loop.LoopUnavailable("default branch is unavailable")
        accepted_main = self.github.accepted_main_sha(branch)
        document = self.github.accepted_plan_document(accepted_main)
        candidate = plan_lane.parse(document, accepted_main)
        if candidate.packet_id != packet_id:
            raise local_loop.LoopUnavailable("plan packet is not the current route")
        ledger = self.github.plan_ledger_issue()
        if type(ledger) is not int or ledger <= 0:
            raise local_loop.LoopUnavailable("plan execution ledger is unavailable")
        return candidate, ledger

    def _wait_for_plan_claim(
        self, ledger_issue: int, dispatch_id: str
    ) -> dict[str, Any] | None:
        deadline = time.monotonic() + self.claim_timeout_seconds
        while True:
            try:
                state = state_manager.read_dispatch_state(
                    ledger_issue, dispatch_id, self.repository
                )
            except state_manager.StateUnavailableError:
                state = None
            if isinstance(state, dict):
                if state.get("status") == "dispatched":
                    details = state.get("details")
                    return details if isinstance(details, dict) else None
                if state.get("status") in {"failed", "failed_unknown_output", "rejected"}:
                    return None
            if time.monotonic() >= deadline:
                return None
            self.sleeper(self.poll_interval_seconds)

    def _wait_for_plan_handoff(
        self,
        ledger_issue: int,
        packet_id: str,
        attempt: str,
        claim_nonce: str,
        pr_number: int,
        head_sha: str,
    ) -> tuple[bool, str]:
        deadline = time.monotonic() + self.claim_timeout_seconds
        while True:
            try:
                worker = state_manager.read_worker_state(ledger_issue, self.repository)
                acquisition = state_manager.read_ci_acquisition(
                    ledger_issue, pr_number, head_sha, self.repository
                )
                run_id = acquisition.get("workflow_run_id") if isinstance(acquisition, dict) else None
                receipt = (
                    state_manager.read_dispatch_state(
                        ledger_issue, f"ci-monitor:{pr_number}:{head_sha}:{run_id}", self.repository
                    )
                    if type(run_id) is int and run_id > 0
                    else None
                )
            except state_manager.StateUnavailableError:
                worker = acquisition = receipt = None
                run_id = None
            extra = worker.get("extra") if isinstance(worker, dict) else None
            if (
                isinstance(worker, dict)
                and worker.get("worker_type") == "plan-run"
                and worker.get("pr_number") == pr_number
                and worker.get("head_sha") == head_sha
                and isinstance(extra, dict)
                and extra.get("subject_id") == packet_id
                and extra.get("attempt_id") == attempt
                and extra.get("claim_nonce") == claim_nonce
                and isinstance(acquisition, dict)
                and acquisition.get("status") == "bound"
                and acquisition.get("workflow_run_id") == run_id
                and isinstance(receipt, dict)
                and receipt.get("action") == "ci-monitor"
                and receipt.get("status") == "dispatched"
            ):
                return True, "handoff_proven"
            if time.monotonic() >= deadline:
                return False, "handoff_state_unproven"
            self.sleeper(self.poll_interval_seconds)

    def _request_plan_handoff(
        self,
        ledger_issue: int,
        packet_id: str,
        attempt: str,
        claim_nonce: str,
        pr_number: int,
        head_sha: str,
    ) -> tuple[bool, str]:
        try:
            self.github.dispatch_controller(
                "handoff-plan",
                {
                    "packet_id": packet_id,
                    "attempt_id": attempt,
                    "head_sha": head_sha,
                    "claim_nonce": claim_nonce,
                },
            )
        except local_loop.LoopUnavailable:
            return False, "handoff_dispatch_outcome_unknown"
        return self._wait_for_plan_handoff(
            ledger_issue, packet_id, attempt, claim_nonce, pr_number, head_sha
        )

    def _read_plan_promotion(
        self, ledger_issue: int, packet_id: str, attempt: str
    ) -> dict[str, Any] | None:
        """Read the promotion or escalation receipt for one closed-out subject."""

        for receipt_id in (
            f"plan-promote:{packet_id}:{attempt}",
            f"plan-escalate:{packet_id}:{attempt}",
        ):
            try:
                state = state_manager.read_dispatch_state(
                    ledger_issue, receipt_id, self.repository
                )
            except state_manager.StateUnavailableError:
                return None
            if isinstance(state, dict):
                return {
                    "kind": state.get("action"),
                    "status": state.get("status"),
                    "details": state.get("details"),
                }
        return None

    @staticmethod
    def _promotion_receipt_classification(receipt: dict[str, Any] | None) -> str:
        """Classify one exact terminal route receipt without widening it."""

        if receipt is None:
            return "pending"
        if receipt.get("kind") == "plan-promote" and receipt.get("status") == "promoted":
            return "promoted"
        if receipt.get("kind") == "plan-escalate" and receipt.get("status") == "escalated":
            return "escalated"
        return "invalid"

    def _promotion_escalation_pause(
        self,
        packet_id: str,
        attempt: str,
        promotion: dict[str, Any],
        **details: object,
    ) -> local_loop.LocalRunOnceResult:
        receipt_details = promotion.get("details")
        reason = receipt_details.get("reason") if isinstance(receipt_details, dict) else None
        return self._plan_result(
            "bounded_pause", packet_id, attempt,
            reason=str(reason or "promotion_escalated"), promotion=promotion, **details,
        )

    def _wait_for_plan_terminal_receipts(
        self,
        ledger_issue: int,
        packet_id: str,
        attempt: str,
        pr_number: int,
        head_sha: str,
    ) -> local_loop.LocalRunOnceResult:
        """Wait for the controller-owned terminal receipts on the ledger.

        CI and review are read back from the existing owners' own ledger
        recordings; merge and closeout are requested through the controller,
        which verifies authoritative GitHub/PR state before recording.  After
        the four terminal receipts, exactly one successor-promotion or
        bounded escalation receipt is requested through the controller and
        read back.  This wait never writes ledger state itself, never runs
        the model again, and never treats a timeout as success for the
        terminal stages.
        """

        deadline = time.monotonic() + self.lifecycle_timeout_seconds
        dispatched_stages: set[str] = set()
        promotion_dispatched = False
        while True:
            lifecycle = plan_lifecycle.read_plan_lifecycle(
                ledger_issue, packet_id, attempt, self.repository
            )
            stages = lifecycle.get("stages") if isinstance(lifecycle, dict) else None
            if isinstance(stages, dict) and all(stages.get(name) for name in ("ci", "review", "merge", "closeout")):
                transitions = lifecycle.get("transitions") or {}
                merge = transitions.get("merge") or {}
                closeout = transitions.get("closeout") or {}
                promotion = self._read_plan_promotion(ledger_issue, packet_id, attempt)
                promotion_classification = self._promotion_receipt_classification(promotion)
                if promotion_classification == "escalated":
                    return self._promotion_escalation_pause(
                        packet_id, attempt, promotion,
                        ledger_issue=ledger_issue, pr_number=pr_number, head_sha=head_sha,
                    )
                if promotion_classification == "invalid":
                    return self._plan_result(
                        "rejected", packet_id, attempt,
                        reason="promotion_receipt_invalid", promotion=promotion,
                    )
                if promotion is None and not promotion_dispatched:
                    try:
                        self.github.dispatch_controller(
                            "promote-plan",
                            {"packet_id": packet_id, "attempt_id": attempt},
                        )
                        promotion_dispatched = True
                    except local_loop.LoopUnavailable:
                        pass
                if promotion_classification == "promoted" or time.monotonic() >= deadline:
                    return self._plan_result(
                        "closed_out", packet_id, attempt,
                        ledger_issue=ledger_issue,
                        pr_number=pr_number, head_sha=head_sha,
                        merge_commit_sha=merge.get("merge_commit_sha"),
                        terminal_packet_state=closeout.get("terminal_packet_state"),
                        closeout_reference=closeout.get("closeout_reference"),
                        promotion=promotion or {},
                        promotion_pending=promotion_classification == "pending",
                    )
            else:
                pending = next(
                    (name for name in ("ci", "review", "merge", "closeout")
                     if not (isinstance(stages, dict) and stages.get(name))),
                    "closeout",
                )
                if pending in {"merge", "closeout"} and pending not in dispatched_stages:
                    try:
                        self.github.dispatch_controller(
                            "lifecycle-plan",
                            {"packet_id": packet_id, "attempt_id": attempt, "stage": pending},
                        )
                        dispatched_stages.add(pending)
                    except local_loop.LoopUnavailable:
                        pass
                if time.monotonic() >= deadline:
                    return self._plan_result(
                        "outcome_unknown", packet_id, attempt,
                        reason="lifecycle_timeout", stage=pending,
                        pr_number=pr_number, head_sha=head_sha,
                    )
            self.sleeper(self.poll_interval_seconds)

    def _unknown_plan_output(
        self, packet_id: str, attempt: str, source_main_sha: str, claim_nonce: str, reason: str
    ) -> local_loop.LocalRunOnceResult:
        try:
            ledger_issue = self.github.plan_ledger_issue()
        except (AttributeError, local_loop.LoopUnavailable):
            return self._plan_result("outcome_unknown", packet_id, attempt, reason=reason)
        dispatch_id = f"plan-run:{packet_id}:{source_main_sha}:{attempt}"
        try:
            self.github.dispatch_controller(
                "block-plan",
                {
                    "packet_id": packet_id,
                    "attempt_id": attempt,
                    "source_main_sha": source_main_sha,
                    "claim_nonce": claim_nonce,
                },
            )
        except local_loop.LoopUnavailable:
            return self._plan_result("outcome_unknown", packet_id, attempt, reason=reason)
        deadline = time.monotonic() + min(self.claim_timeout_seconds, 30)
        while True:
            try:
                state = state_manager.read_dispatch_state(
                    ledger_issue, dispatch_id, self.repository
                )
            except state_manager.StateUnavailableError:
                state = None
            details = state.get("details") if isinstance(state, dict) else None
            if (
                isinstance(state, dict)
                and state.get("action") == "plan-run"
                and state.get("status") == "failed_unknown_output"
                and isinstance(details, dict)
                and details.get("subject_kind") == "plan-packet"
                and details.get("subject_id") == packet_id
                and details.get("source_main_sha") == source_main_sha
                and details.get("claim_nonce") == claim_nonce
                and details.get("reason") == "local_unknown_output"
            ):
                return self._plan_result("failed_unknown_output", packet_id, attempt, reason=reason)
            if time.monotonic() >= deadline:
                return self._plan_result("outcome_unknown", packet_id, attempt, reason=reason)
            self.sleeper(self.poll_interval_seconds)

    def run_plan_once(self, packet_id: str, attempt_id: str) -> local_loop.LocalRunOnceResult:
        """Execute one exact plan subject only when terminal owners are ready.

        The blanket deferral is replaced by real fail-closed readiness checks:
        the Plan Execution Ledger, canonical CI workflow, CI monitor workflow,
        review owner, repository-maintenance merge owner, and canonical
        closeout owner must all be provably usable for the plan subject before
        any claim or mutation is attempted.  A missing owner rejects the
        attempt with the specific owner list; an already-dispatched generation
        is recovered idempotently from the ledger plus authoritative
        GitHub/PR/CI/review state, never re-executed.
        """

        attempt = _canonical_attempt_id(attempt_id)
        if not isinstance(packet_id, str) or not plan_lane.PACKET_ID.fullmatch(packet_id):
            return self._plan_result("rejected", str(packet_id), str(attempt_id), reason="invalid_plan_id")
        if attempt is None:
            return self._plan_result("rejected", packet_id, str(attempt_id), reason="invalid_attempt_id")
        try:
            control = self.github.read_control_state()
            if control.get("emergency_stop") or not control.get("orchestrator_enabled"):
                return self._plan_result("control_stopped", packet_id, attempt)
            metadata = self.github.repository_metadata()
            if str(metadata.get("name_with_owner", "")).casefold() != self.repository.casefold():
                return self._plan_result("identity_rejected", packet_id, attempt, reason="repository_identity_mismatch")
            default_branch = metadata.get("default_branch")
            if not isinstance(default_branch, str) or not local_loop.BRANCH.fullmatch(default_branch):
                return self._plan_result("unavailable", packet_id, attempt, reason="default_branch_unavailable")
            accepted_main = self.github.accepted_main_sha(default_branch)
            local_main = self.git.origin_main_sha(self.repo_path, default_branch)
            if not local_loop.HEX40.fullmatch(accepted_main) or accepted_main != local_main:
                return self._plan_result(
                    "stale_checkout", packet_id, attempt,
                    accepted_main_sha=accepted_main, local_origin_main_sha=local_main,
                )
            ready, missing = self._plan_terminal_owner_readiness()
            if not ready:
                return self._plan_result(
                    "rejected", packet_id, attempt,
                    reason=f"plan_lane_not_ready:{','.join(missing)}",
                )
            candidate, ledger_issue = self._live_plan(packet_id)
            recovered = self._recover_existing_plan_claim(
                packet_id, attempt, candidate, ledger_issue
            )
            if recovered is not None:
                if recovered.get("status") == "handed_off":
                    pr_number = recovered.get("pr_number")
                    head_sha = recovered.get("head_sha")
                    if (
                        type(pr_number) is int and pr_number > 0
                        and isinstance(head_sha, str)
                        and local_loop.HEX40.fullmatch(head_sha) is not None
                    ):
                        return self._wait_for_plan_terminal_receipts(
                            ledger_issue, packet_id, attempt, pr_number, head_sha
                        )
                return recovered
            return self._run_plan_once_authorized(packet_id, attempt)
        except (local_loop.LoopUnavailable, plan_lane.PlanLaneError) as exc:
            return self._plan_result("unavailable", packet_id, attempt, reason=str(exc)[:200])

    def reconcile_plan(self, packet_id: str) -> local_loop.LocalRunOnceResult | None:
        """Resume the one exact active plan generation for the current packet.

        A route restart must not mint a fresh attempt while a prior claim is
        live.  The Plan Execution Ledger remains the only source for this
        lookup; an absent generation returns ``None`` so the normal claim path
        may start, while two distinct active or just-closed generations fail
        closed.  A closed generation is deliberately returned as ``closed_out``
        rather than re-entered through ``run_plan_once``: the route driver must
        resume its exact promotion attempt, not claim the old packet again
        while the promotion PR is still in flight.
        """

        if not isinstance(packet_id, str) or not plan_lane.PACKET_ID.fullmatch(packet_id):
            return self._plan_result("rejected", str(packet_id), "", reason="invalid_plan_id")
        try:
            candidate, ledger_issue = self._live_plan(packet_id)
            comments = state_manager.get_issue_comments(ledger_issue, self.repository)
        except (local_loop.LoopUnavailable, plan_lane.PlanLaneError, state_manager.StateUnavailableError):
            return self._plan_result("unavailable", packet_id, "", reason="plan_reconcile_unavailable")
        generations: list[tuple[str, str, dict[str, Any]]] = []
        seen_dispatches: set[str] = set()
        for comment in comments:
            if (comment.get("author") or {}).get("login") not in state_manager.TRUSTED_STATE_AUTHORS:
                continue
            try:
                state = json.loads(comment.get("body", ""))
            except (TypeError, json.JSONDecodeError):
                continue
            if not isinstance(state, dict) or state.get("action") != "plan-run":
                continue
            dispatch_id = state.get("dispatch_id")
            details = state.get("details")
            if not isinstance(dispatch_id, str) or dispatch_id in seen_dispatches:
                continue
            seen_dispatches.add(dispatch_id)
            if (
                not isinstance(details, dict)
                or details.get("subject_kind") != "plan-packet"
                or details.get("subject_id") != packet_id
                or details.get("source_main_sha") != candidate.source_main_sha
            ):
                continue
            if state.get("status") in {"failed_unknown_output", "outcome_unknown"}:
                return self._plan_result(
                    "outcome_unknown", packet_id, "",
                    reason="plan_reconcile_outcome_unknown",
                )
            if state.get("status") not in {"claimed", "dispatched", "closed_out"}:
                # A terminal provider-free worker failure is released by the
                # existing controller and permits one fresh generation.  It is
                # intentionally distinct from the non-retryable unknown case
                # above.
                continue
            attempt = details.get("attempt_id")
            if _canonical_attempt_id(attempt) is None:
                return self._plan_result("rejected", packet_id, "", reason="plan_reconcile_binding_invalid")
            generations.append((state["status"], attempt, details))
        if not generations:
            return None
        if len(generations) != 1:
            return self._plan_result("rejected", packet_id, "", reason="plan_reconcile_ambiguous")
        status, attempt, details = generations[0]
        if status == "closed_out":
            return self._plan_result(
                "closed_out",
                packet_id,
                attempt,
                terminal_packet_state=details.get("terminal_packet_state"),
                closeout_reference=details.get("closeout_reference"),
                reconciled=True,
            )
        return self.run_plan_once(packet_id, attempt)

    def _plan_terminal_owner_readiness(self) -> tuple[bool, list[str]]:
        """Read-only provider-free proof of terminal-owner readiness."""

        workflow_dir = Path(self.repo_path) / ".github" / "workflows"
        canonical_tests = workflow_dir / "tests.yml"
        ci_monitor = workflow_dir / "agent-ci-monitor.yml"
        review_owner = Path(self.repo_path) / "scripts" / "agent-control" / "review_loop_cli.py"
        merge_owner = Path(self.repo_path) / "docs" / "REAL_WORLD_TESTING_PLAYBOOK.md"
        closeout_owner = Path(self.repo_path) / "docs" / "CURRENT_STATUS.md"
        ledger_issue = 0
        try:
            ledger_issue = self.github.plan_ledger_issue()
        except (AttributeError, local_loop.LoopUnavailable):
            ledger_issue = 0
        return plan_lane.terminal_owner_readiness(
            ledger_issue=ledger_issue,
            canonical_tests_workflow_present=canonical_tests.is_file(),
            ci_monitor_workflow_present=ci_monitor.is_file(),
            review_owner_present=review_owner.is_file(),
            merge_owner_present=merge_owner.is_file(),
            closeout_owner_present=closeout_owner.is_file(),
        )

    def _recover_existing_plan_claim(
        self,
        packet_id: str,
        attempt: str,
        candidate: plan_lane.PlanCandidate,
        ledger_issue: int,
    ) -> local_loop.LocalRunOnceResult | None:
        """Reconstruct a provable exact-head plan handoff; never recreate output.

        Returns a result only when an existing dispatched plan-run generation
        can be repaired from the ledger plus authoritative GitHub/PR/CI state.
        A claimed-but-not-dispatched generation is re-entered through the
        controller; a terminal generation is reported as terminal; any
        unprovable state returns ``None`` so the caller executes fresh.
        """

        dispatch_id = f"plan-run:{packet_id}:{candidate.source_main_sha}:{attempt}"
        try:
            existing = state_manager.read_dispatch_state(
                ledger_issue, dispatch_id, self.repository
            )
        except state_manager.StateUnavailableError:
            return None
        if not isinstance(existing, dict):
            return None
        status = existing.get("status")
        details = existing.get("details")
        if not isinstance(details, dict):
            if status in {"failed", "failed_unknown_output", "closed_out", "rejected", "outcome_unknown"}:
                return self._plan_result("terminal", packet_id, attempt, claim_status=status)
            return None
        if status == "closed_out":
            return self._plan_result(
                "terminal", packet_id, attempt, claim_status=status,
                terminal_packet_state=details.get("terminal_packet_state"),
                closeout_reference=details.get("closeout_reference"),
            )
        if status == "dispatched":
            # A dispatched generation may never fall through to a second
            # execution.  If its binding cannot be proven (stale lease,
            # mismatched attempt, missing nonce), the generation stays in
            # flight and fail-closed reconciliation owns the ledger state.
            valid, _reason = state_manager.plan_claim_binding_valid(
                ledger_issue,
                details,
                packet_id,
                attempt,
                local_loop.plan_execution_token(
                    self.repository, packet_id, candidate.source_main_sha, attempt
                ),
                candidate.source_main_sha,
                candidate.task_spec_sha256,
            )
            if not valid:
                return self._plan_result(
                    "in_flight", packet_id, attempt, dispatch_id=dispatch_id,
                    reason="dispatched_generation_unverifiable",
                )
            repaired = self._repair_plan_handoff(
                packet_id, attempt, candidate, ledger_issue, details
            )
            if repaired is not None:
                return repaired
            return self._plan_result(
                "in_flight", packet_id, attempt, dispatch_id=dispatch_id,
                reason="dispatched_generation_unrepairable",
            )
        if status == "claimed":
            # Claim persisted but dispatch promotion may have crashed.  Re-enter
            # the authorized path: claim-plan resumes claimed→dispatched and the
            # same exact generation continues, never a second execution after a
            # provable handoff.
            return None
        if status in {"failed", "failed_unknown_output", "rejected", "outcome_unknown"}:
            return self._plan_result("terminal", packet_id, attempt, claim_status=status)
        return None

    def _repair_plan_handoff(
        self,
        packet_id: str,
        attempt: str,
        candidate: plan_lane.PlanCandidate,
        ledger_issue: int,
        details: dict[str, Any],
    ) -> local_loop.LocalRunOnceResult | None:
        """Repair only a provable exact-head plan handoff; never recreate output."""

        branch = details.get("canonical_branch")
        if branch != candidate.branch:
            return None
        try:
            remote = self._git_checked(
                self.repo_path, "ls-remote", "origin", f"refs/heads/{branch}"
            )
        except local_loop.LoopUnavailable:
            return self._plan_result("outcome_unknown", packet_id, attempt, reason="remote_head_unavailable")
        remote_parts = remote.split()
        if remote_parts and (
            len(remote_parts) != 2 or remote_parts[1] != f"refs/heads/{branch}"
        ):
            return self._plan_result("outcome_unknown", packet_id, attempt, reason="remote_head_ambiguous")
        head_sha = remote_parts[0] if remote_parts else ""
        if head_sha and not local_loop.HEX40.fullmatch(head_sha):
            return self._plan_result("outcome_unknown", packet_id, attempt, reason="remote_head_invalid")
        if not local_loop.HEX40.fullmatch(head_sha) or head_sha == candidate.source_main_sha:
            return None
        try:
            pr = pr_binding.find_plan_pr(
                packet_id, branch, head_sha, candidate.source_main_sha,
                candidate.task_spec_sha256, self.repository,
            )
        except pr_binding.PRBindingError:
            try:
                marker = json.dumps({
                    "subject_kind": "plan-packet", "subject_id": packet_id,
                    "source_main_sha": candidate.source_main_sha,
                    "task_spec_sha256": candidate.task_spec_sha256,
                    "branch": branch,
                }, sort_keys=True)
                pr = pr_binding.create_or_update_plan_pr(
                    packet_id, branch, head_sha, candidate.source_main_sha,
                    candidate.task_spec_sha256, candidate.goal[:200],
                    f"<!-- agent-orchestrator-binding: {marker} -->\n\nPlan packet `{packet_id}`.",
                    self.repository,
                )
                pr_binding.verify_post_push_plan_binding(
                    packet_id, pr.get("number") or 0, branch, head_sha,
                    candidate.source_main_sha, candidate.task_spec_sha256, self.repository,
                )
            except (local_loop.LoopUnavailable, pr_binding.PRBindingError, KeyError, TypeError):
                return None
        pr_number = pr.get("number")
        if type(pr_number) is not int:
            return None
        claim_nonce = details.get("claim_nonce")
        if not isinstance(claim_nonce, str) or state_manager.CLAIM_NONCE_PATTERN.fullmatch(claim_nonce) is None:
            return None
        handed_off, handoff_reason = self._request_plan_handoff(
            ledger_issue, packet_id, attempt, claim_nonce, pr_number, head_sha
        )
        if not handed_off:
            return self._plan_result("outcome_unknown", packet_id, attempt, reason=handoff_reason)
        return self._plan_result(
            "handed_off", packet_id, attempt,
            pr_number=pr_number, head_sha=head_sha, branch=branch,
            accepted_main_sha=candidate.source_main_sha,
        )

    def _run_plan_once_authorized(self, packet_id: str, attempt_id: str) -> local_loop.LocalRunOnceResult:
        """Internal plan path retained for unit tests of deferred infrastructure."""

        attempt = _canonical_attempt_id(attempt_id)
        if not isinstance(packet_id, str) or not plan_lane.PACKET_ID.fullmatch(packet_id):
            return self._plan_result("rejected", str(packet_id), str(attempt_id), reason="invalid_plan_id")
        if attempt is None:
            return self._plan_result("rejected", packet_id, str(attempt_id), reason="invalid_attempt_id")
        worktree_path: Path | None = None
        pushed = False
        unknown_output = False
        candidate: plan_lane.PlanCandidate | None = None
        ledger_issue: int | None = None
        claim_nonce = ""
        try:
            control = self.github.read_control_state()
            if control.get("emergency_stop") or not control.get("orchestrator_enabled"):
                return self._plan_result("control_stopped", packet_id, attempt)
            metadata = self.github.repository_metadata()
            if str(metadata.get("name_with_owner", "")).casefold() != self.repository.casefold():
                return self._plan_result("identity_rejected", packet_id, attempt, reason="repository_identity_mismatch")
            default_branch = metadata.get("default_branch")
            if not isinstance(default_branch, str) or not local_loop.BRANCH.fullmatch(default_branch):
                return self._plan_result("unavailable", packet_id, attempt, reason="default_branch_unavailable")
            accepted_main = self.github.accepted_main_sha(default_branch)
            local_main = self.git.origin_main_sha(self.repo_path, default_branch)
            if not local_loop.HEX40.fullmatch(accepted_main) or accepted_main != local_main:
                return self._plan_result(
                    "stale_checkout", packet_id, attempt,
                    accepted_main_sha=accepted_main, local_origin_main_sha=local_main,
                )
            candidate, ledger_issue = self._live_plan(packet_id)
            dispatch_id = f"plan-run:{packet_id}:{candidate.source_main_sha}:{attempt}"
            self.github.dispatch_controller(
                "claim-plan", {"packet_id": packet_id, "attempt_id": attempt}
            )
            details = self._wait_for_plan_claim(ledger_issue, dispatch_id)
            if details is None:
                return self._plan_result("claim_unavailable", packet_id, attempt)
            claim_nonce = details.get("claim_nonce", "")
            token = local_loop.plan_execution_token(
                self.repository, packet_id, candidate.source_main_sha, attempt
            )
            valid, reason = state_manager.plan_claim_binding_valid(
                ledger_issue, details, packet_id, attempt, token,
                candidate.source_main_sha, candidate.task_spec_sha256,
            )
            if not valid:
                return self._plan_result("claim_rejected", packet_id, attempt, reason=reason)
            if details.get("allowed_paths") != candidate.allowed_paths:
                return self._plan_result("claim_rejected", packet_id, attempt, reason="plan_scope_changed")
            created = worktree_manager.create_plan_worktree(
                packet_id, candidate.branch, str(self.repo_path), candidate.source_main_sha
            )
            if not created:
                return self._plan_result("failed", packet_id, attempt, reason="worktree_failed")
            worktree_path = Path(created[0])
            base_sha, expected_remote_sha = created[2], created[3]
            artifact_dir = self._owned_artifact_dir(
                packet_id, attempt, candidate.branch, base_sha, claim_nonce
            )
            with tempfile.TemporaryDirectory(prefix="agent-plan-run-") as temp:
                temp_dir = Path(temp)
                prompt_file = temp_dir / "implementation-prompt.txt"
                prompt_file.write_text(
                    prompt_builder.build_claim_bound_plan_implementation_prompt(
                        packet_id, candidate.goal, candidate.allowed_paths,
                        candidate.source_main_sha, candidate.branch,
                        prerequisites=candidate.prerequisites,
                        forbidden_changes=candidate.forbidden_changes,
                        verification=candidate.verification,
                        rollback=candidate.rollback,
                        repo_root=self.repo_path,
                    ),
                    encoding="utf-8",
                )
                output_dir = temp_dir / "codex-output"
                wrapper = Path(__file__).resolve().parent / "codex_wrapper.sh"
                exit_code, _stdout, _stderr = _bounded_process(
                    ["bash", str(wrapper), "implement", str(prompt_file), str(output_dir), str(worktree_path)],
                    timeout_seconds=self.command_timeout_seconds,
                )
                if exit_code != 0:
                    return self._plan_result("failed", packet_id, attempt, reason="codex_failed")
                exit_file = output_dir / "codex-exit-code.txt"
                if not exit_file.is_file() or exit_file.read_text().strip() != "0":
                    return self._plan_result("failed", packet_id, attempt, reason="codex_result_invalid")
                try:
                    local_checks = local_verification.run_plan_focused_checks(
                        worktree_path, candidate.verification
                    )
                except local_verification.LocalVerificationError as exc:
                    return self._plan_result(
                        "failed", packet_id, attempt, reason=str(exc.reason)[:200]
                    )
                manifest = artifact_contract.create_artifact(
                    repo=worktree_path, artifact_dir=artifact_dir, worker_type="implementation",
                    issue_number=0, pr_number=0, base_sha=base_sha,
                    expected_remote_sha=expected_remote_sha, branch=candidate.branch,
                    codex_exit_code=0, local_checks=local_checks,
                    subject_kind="plan-packet", subject_id=packet_id,
                )
                artifact_contract.validate_artifact(
                    artifact_dir=artifact_dir, expected_worker_type="implementation",
                    issue_number=0, pr_number=0, base_sha=base_sha,
                    expected_remote_sha=expected_remote_sha, branch=candidate.branch,
                    subject_kind="plan-packet", subject_id=packet_id,
                )
                self._git_checked(worktree_path, "reset", "--hard", base_sha)
                self._git_checked(worktree_path, "clean", "-fd")
                self._git_checked(worktree_path, "apply", "--index", "--binary", str(artifact_dir / artifact_contract.PATCH_NAME))
                artifact_contract.validate_index(worktree_path, manifest)
                self._git_checked(worktree_path, "diff", "--check")
                self._git_checked(worktree_path, "commit", "-m", f"feat: implement plan packet {packet_id}")
                head_sha = self._git_checked(worktree_path, "rev-parse", "HEAD")
                if not local_loop.HEX40.fullmatch(head_sha):
                    return self._plan_result("failed", packet_id, attempt, reason="commit_sha_invalid")
                push_args = ["push"]
                if expected_remote_sha:
                    push_args.append(f"--force-with-lease=refs/heads/{candidate.branch}:{expected_remote_sha}")
                push_args.extend(["origin", f"HEAD:refs/heads/{candidate.branch}"])
                push_code, _stdout, _stderr = _bounded_process(
                    ["git", *push_args], cwd=worktree_path, timeout_seconds=120
                )
                try:
                    remote = self._git_checked(
                        self.repo_path, "ls-remote", "origin", f"refs/heads/{candidate.branch}"
                    )
                except local_loop.LoopUnavailable:
                    unknown_output = True
                    return self._unknown_plan_output(packet_id, attempt, candidate.source_main_sha, claim_nonce, "remote_head_unavailable")
                parts = remote.split()
                remote_head = parts[0] if parts and len(parts) == 2 else None
                if remote_head == head_sha:
                    pushed = True
                elif remote_head == expected_remote_sha or (remote_head is None and expected_remote_sha is None):
                    return self._plan_result("failed", packet_id, attempt, reason="push_not_applied")
                else:
                    unknown_output = True
                    return self._unknown_plan_output(packet_id, attempt, candidate.source_main_sha, claim_nonce, "push_outcome_unknown")
                if push_code != 0 and not pushed:
                    return self._plan_result("failed", packet_id, attempt, reason="push_not_applied")
                marker = json.dumps({
                    "subject_kind": "plan-packet", "subject_id": packet_id,
                    "source_main_sha": candidate.source_main_sha,
                    "task_spec_sha256": candidate.task_spec_sha256,
                    "branch": candidate.branch,
                }, sort_keys=True)
                pr = pr_binding.create_or_update_plan_pr(
                    packet_id, candidate.branch, head_sha, candidate.source_main_sha,
                    candidate.task_spec_sha256, candidate.goal[:200],
                    f"<!-- agent-orchestrator-binding: {marker} -->\n\nPlan packet `{packet_id}`.",
                    self.repository,
                )
                pr_number = pr.get("number")
                if type(pr_number) is not int:
                    unknown_output = True
                    return self._unknown_plan_output(packet_id, attempt, candidate.source_main_sha, claim_nonce, "pr_number_unavailable")
                pr_binding.verify_post_push_plan_binding(
                    packet_id, pr_number, candidate.branch, head_sha,
                    candidate.source_main_sha, candidate.task_spec_sha256, self.repository,
                )
                self.github.dispatch_controller(
                    "handoff-plan",
                    {"packet_id": packet_id, "attempt_id": attempt, "head_sha": head_sha, "claim_nonce": claim_nonce},
                )
                handed_off, handoff_reason = self._wait_for_plan_handoff(
                    ledger_issue, packet_id, attempt, claim_nonce, pr_number, head_sha
                )
                if not handed_off:
                    unknown_output = True
                    return self._unknown_plan_output(packet_id, attempt, candidate.source_main_sha, claim_nonce, handoff_reason)
                return self._wait_for_plan_terminal_receipts(
                    ledger_issue, packet_id, attempt, pr_number, head_sha
                )
        except (local_loop.LoopUnavailable, artifact_contract.ArtifactContractError, pr_binding.PRBindingError, OSError, ValueError) as exc:
            if pushed and candidate is not None and claim_nonce:
                unknown_output = True
                return self._unknown_plan_output(packet_id, attempt, candidate.source_main_sha, claim_nonce, "external_outcome_unknown")
            return self._plan_result("failed", packet_id, attempt, reason=str(exc)[:200])
        finally:
            if worktree_path is not None:
                worktree_manager.remove_plan_worktree(packet_id, str(self.repo_path), candidate.branch if candidate else "")
            if candidate is not None and ledger_issue is not None and claim_nonce and not pushed and not unknown_output:
                try:
                    self.github.dispatch_controller(
                        "release-plan",
                        {
                            "packet_id": packet_id,
                            "attempt_id": attempt,
                            "source_main_sha": candidate.source_main_sha,
                            "reason_code": "local_environment_failure",
                            "claim_nonce": claim_nonce,
                        },
                    )
                except local_loop.LoopUnavailable:
                    pass

    def run_route_once(self, packet_id: str, attempt_id: str) -> local_loop.LocalRunOnceResult:
        """Drive the canonical closeout/promotion PR lifecycle for one closed packet.

        After an accepted plan closeout, the deterministic route layer selects
        one inventory successor, then a read-only weak planner proposes the
        refreshed facts from the exact accepted tree.  The route verifier must
        independently prove every owner/caller/test/path/decision fact before
        it compiles the replace-only routing diff (NEXT_DECISION, FUTURE_ROUTE,
        CURRENT_STATUS) and opens one Draft promotion PR.  FUTURE_ROUTE prose
        is never used as edit authority.  An EFFECT compiles only its bounded
        preparation and typed T3 request; it is never executed here.  Resume
        verifies the existing exact-head PR and its manual merge readback;
        this adapter never merges, creates a duplicate PR, or treats a missing
        receipt as success.
        """

        attempt = _canonical_attempt_id(attempt_id)
        if not isinstance(packet_id, str) or not plan_lane.PACKET_ID.fullmatch(packet_id):
            return self._plan_result("rejected", str(packet_id), str(attempt_id), reason="invalid_plan_id")
        if attempt is None:
            return self._plan_result("rejected", packet_id, str(attempt_id), reason="invalid_attempt_id")
        try:
            control = self.github.read_control_state()
            if control.get("emergency_stop") or not control.get("orchestrator_enabled"):
                return self._plan_result("control_stopped", packet_id, attempt)
            metadata = self.github.repository_metadata()
            if str(metadata.get("name_with_owner", "")).casefold() != self.repository.casefold():
                return self._plan_result("identity_rejected", packet_id, attempt, reason="repository_identity_mismatch")
            default_branch = metadata.get("default_branch")
            if not isinstance(default_branch, str) or not local_loop.BRANCH.fullmatch(default_branch):
                return self._plan_result("unavailable", packet_id, attempt, reason="default_branch_unavailable")
            accepted_main = self.github.accepted_main_sha(default_branch)
            local_main = self.git.origin_main_sha(self.repo_path, default_branch)
            if not local_loop.HEX40.fullmatch(accepted_main) or accepted_main != local_main:
                return self._plan_result(
                    "stale_checkout", packet_id, attempt,
                    accepted_main_sha=accepted_main, local_origin_main_sha=local_main,
                )
        except (local_loop.LoopUnavailable, AttributeError):
            return self._plan_result("unavailable", packet_id, attempt, reason="control_state_unavailable")
        try:
            ledger_issue = self.github.plan_ledger_issue()
        except local_loop.LoopUnavailable:
            return self._plan_result("unavailable", packet_id, attempt, reason="plan_ledger_unavailable")
        claim = plan_lifecycle._exact_plan_claim(ledger_issue, packet_id, attempt, self.repository)
        if claim is None:
            return self._plan_result("rejected", packet_id, attempt, reason="plan_claim_not_found")
        if claim.get("status") != "closed_out":
            return self._plan_result("rejected", packet_id, attempt, reason="plan_claim_not_closed_out")
        details = claim.get("details")
        if not isinstance(details, dict) or not isinstance(details.get("closeout_reference"), str):
            closeout_reference = f"merge on accepted main `{accepted_main}`"
        else:
            closeout_reference = details["closeout_reference"].strip()
        try:
            next_document = self.github.accepted_plan_document(accepted_main)
            future_document = self.github.accepted_route_document(accepted_main)
            status_document = self.github.accepted_status_document(accepted_main)
        except local_loop.LoopUnavailable:
            return self._plan_result("unavailable", packet_id, attempt, reason="routing_documents_unavailable")
        try:
            _successor_id, _digest = plan_lane.successor_binding(next_document, packet_id, accepted_main)
            try:
                self.github.dispatch_controller(
                    "promote-plan", {"packet_id": packet_id, "attempt_id": attempt}
                )
            except local_loop.LoopUnavailable:
                return self._plan_result(
                    "successor_current", packet_id, attempt,
                    accepted_main_sha=accepted_main, successor_id=_successor_id,
                )
            promotion = self._read_plan_promotion(ledger_issue, packet_id, attempt)
            promotion_classification = self._promotion_receipt_classification(promotion)
            if promotion_classification == "escalated":
                return self._promotion_escalation_pause(
                    packet_id, attempt, promotion,
                    accepted_main_sha=accepted_main, successor_id=_successor_id,
                )
            if promotion_classification == "invalid":
                return self._plan_result(
                    "rejected", packet_id, attempt,
                    accepted_main_sha=accepted_main, successor_id=_successor_id,
                    reason="promotion_receipt_invalid", promotion=promotion,
                )
            return self._plan_result(
                "promoted" if promotion_classification == "promoted" else "promotion_pending",
                packet_id, attempt,
                accepted_main_sha=accepted_main, successor_id=_successor_id,
                promotion=promotion or {},
            )
        except plan_lane.PlanLaneError as exc:
            if exc.reason not in {"plan_packet_absent", "successor_still_current", "multiple_plan_packets"}:
                return self._plan_result("rejected", packet_id, attempt, reason=f"routing_invalid:{exc.reason}")
        try:
            successor = route_driver.eligible_successor(
                future_document,
                next_document,
                packet_id,
                completed_ids=route_driver._accepted_completed_ids(status_document),
            )
            planned = self._plan_current_main_evidence(
                successor, accepted_main, closeout_reference
            )
            if planned.state not in {"READY_FOR_EXECUTION", "T3_REQUIRED"} or planned.evidence is None:
                return self._dispatch_bounded_pause(packet_id, attempt, planned.reason)
            compiled = route_driver.compile_successor(
                future_document, next_document, status_document,
                packet_id, closeout_reference, accepted_main, planned.evidence,
            )
        except route_driver.RouteDriverError as exc:
            return self._dispatch_bounded_pause(packet_id, attempt, exc.reason)
        return self._drive_promotion_pr(packet_id, attempt, accepted_main, ledger_issue, compiled, details)

    def run_effect_route_once(
        self,
        request: route_driver.T3Request,
        receipt: route_driver.T3Receipt,
    ) -> local_loop.LocalRunOnceResult:
        """Resume a completed operator-owned EFFECT through its closeout PR.

        This is deliberately not an effect executor.  The operator invokes the
        already-owned product runtime under the finite T3 authority, then
        records only redacted authority/outcome digests through the controller.
        Here we independently re-read that ledger record and compile the next
        provider-free CLOSEOUT contract.  A crash before such a record leaves
        the T3 node paused; a malformed or changed record never advances it.
        """

        if not isinstance(request, route_driver.T3Request) or not isinstance(
            receipt, route_driver.T3Receipt
        ):
            return self._plan_result("rejected", "", "", reason="route_effect_receipt_invalid")
        attempt = str(uuid.uuid5(
            uuid.NAMESPACE_URL,
            f"route-effect:{self.repository}:{request.packet_id}:{request.candidate_digest}",
        ))
        try:
            control = self.github.read_control_state()
            if control.get("emergency_stop") or not control.get("orchestrator_enabled"):
                return self._plan_result("control_stopped", request.packet_id, attempt)
            metadata = self.github.repository_metadata()
            if str(metadata.get("name_with_owner", "")).casefold() != self.repository.casefold():
                return self._plan_result(
                    "identity_rejected", request.packet_id, attempt,
                    reason="repository_identity_mismatch",
                )
            default_branch = metadata.get("default_branch")
            if not isinstance(default_branch, str) or not local_loop.BRANCH.fullmatch(default_branch):
                return self._plan_result(
                    "unavailable", request.packet_id, attempt,
                    reason="default_branch_unavailable",
                )
            accepted_main = self.github.accepted_main_sha(default_branch)
            local_main = self.git.origin_main_sha(self.repo_path, default_branch)
            if accepted_main != request.accepted_main_sha or accepted_main != local_main:
                return self._plan_result(
                    "stale_checkout", request.packet_id, attempt,
                    accepted_main_sha=accepted_main,
                    receipt_main_sha=request.accepted_main_sha,
                    local_origin_main_sha=local_main,
                )
            current_request = route_driver.current_t3_request(
                self.github.accepted_plan_document(accepted_main), accepted_main
            )
            if current_request != request:
                return self._plan_result(
                    "rejected", request.packet_id, attempt,
                    reason="route_effect_request_not_current",
                )
            ledger_issue = self.github.plan_ledger_issue()
            state = state_manager.read_dispatch_state(
                ledger_issue,
                f"route-t3:{request.packet_id}:{request.candidate_digest}",
                self.repository,
            )
        except (
            local_loop.LoopUnavailable,
            route_driver.RouteDriverError,
            state_manager.StateUnavailableError,
        ):
            return self._plan_result(
                "unavailable", request.packet_id, attempt,
                reason="route_effect_receipt_unavailable",
            )
        expected_receipt = {
            "schema_version": "route_t3_receipt.v1",
            "packet_id": receipt.packet_id,
            "accepted_main_sha": receipt.accepted_main_sha,
            "candidate_digest": receipt.candidate_digest,
            "action_digest": receipt.action_digest,
            "scope_digest": receipt.scope_digest,
            "authority_receipt_digest": receipt.authority_receipt_digest,
            "outcome_receipt_digest": receipt.outcome_receipt_digest,
            "authority_owner_digest": receipt.authority_owner_digest,
            "operator": receipt.operator,
            "issued_at": receipt.issued_at,
            "expires_at": receipt.expires_at,
            "disposition": receipt.disposition,
        }
        if not (
            isinstance(state, dict)
            and state.get("action") == "route-t3-receipt"
            and state.get("status") == "authorized"
            and state.get("details") == expected_receipt
        ):
            return self._plan_result(
                "rejected", request.packet_id, attempt,
                reason="route_effect_receipt_unproved",
            )
        closeout_reference = (
            "T3 operator authority "
            f"`{receipt.authority_receipt_digest}`; redacted effect outcome "
            f"`{receipt.outcome_receipt_digest}`"
        )
        try:
            next_document = self.github.accepted_plan_document(accepted_main)
            future_document = self.github.accepted_route_document(accepted_main)
            status_document = self.github.accepted_status_document(accepted_main)
            successor = route_driver.eligible_successor(
                future_document,
                next_document,
                request.packet_id,
                completed_ids=route_driver._accepted_completed_ids(status_document),
            )
            if (
                successor.profile[1] != "CLOSEOUT"
                or request.packet_id not in successor.sketch.prerequisites
            ):
                return self._plan_result(
                    "outcome_unknown", request.packet_id, attempt,
                    reason="route_effect_closeout_not_proved",
                )
            planned = self._plan_current_main_evidence(
                successor, accepted_main, closeout_reference
            )
            if planned.state not in {"READY_FOR_EXECUTION", "T3_REQUIRED"} or planned.evidence is None:
                return self._plan_result(
                    "bounded_pause", request.packet_id, attempt, reason=planned.reason
                )
            compiled = route_driver.compile_successor(
                future_document,
                next_document,
                status_document,
                request.packet_id,
                closeout_reference,
                accepted_main,
                planned.evidence,
                closed_packet_state="IN_PROGRESS",
            )
        except (local_loop.LoopUnavailable, route_driver.RouteDriverError):
            return self._plan_result(
                "bounded_pause", request.packet_id, attempt,
                reason="route_effect_closeout_evidence_unproved",
            )
        # The promotion PR itself is the canonical durable closeout transition
        # for this operator-owned effect.  There was intentionally no weak
        # plan claim for the EFFECT, so do not fabricate one merely to reuse a
        # ledger receipt shape.
        return self._drive_promotion_pr(
            request.packet_id, attempt, accepted_main, ledger_issue, compiled, {}
        )

    def _plan_current_main_evidence(
        self,
        successor: route_driver.EligibleSuccessor,
        accepted_main: str,
        predecessor_receipt: str,
    ) -> route_driver.PromotionPlanResult:
        """Obtain a bounded proposal then prove it against exact accepted main.

        The worker is a read-only transport only.  Its output is held in a
        temporary directory, never made a receipt, and is useful only if
        ``CurrentMainEvidenceVerifier`` can independently reproduce every
        referenced fact from Git's accepted tree.
        """

        try:
            prompt = route_driver.promotion_planner_prompt(
                successor, accepted_main, predecessor_receipt
            )
        except route_driver.RouteDriverError as exc:
            return route_driver.PromotionPlanResult("DECISION_REQUIRED", exc.reason)
        output: str | None
        if self.promotion_output_provider is not None:
            try:
                output = self.promotion_output_provider(prompt, self.repo_path)
            except (OSError, ValueError):
                return route_driver.PromotionPlanResult(
                    "DECISION_REQUIRED", "promotion_planner_unavailable"
                )
        else:
            try:
                with tempfile.TemporaryDirectory(prefix="route-promotion-plan-") as temporary:
                    temporary_path = Path(temporary)
                    prompt_path = temporary_path / "planner-prompt.txt"
                    output_path = temporary_path / "output"
                    prompt_path.write_text(prompt, encoding="utf-8")
                    wrapper = Path(__file__).resolve().parent / "codex_wrapper.sh"
                    exit_code, _stdout, _stderr = _bounded_process(
                        ["bash", str(wrapper), "review", str(prompt_path), str(output_path), str(self.repo_path)],
                        timeout_seconds=self.command_timeout_seconds,
                    )
                    last_message = output_path / "codex-last-message.txt"
                    if exit_code != 0 or not last_message.is_file():
                        return route_driver.PromotionPlanResult(
                            "DECISION_REQUIRED", "promotion_planner_unavailable"
                        )
                    output = last_message.read_text(encoding="utf-8")
            except (OSError, UnicodeDecodeError):
                return route_driver.PromotionPlanResult(
                    "DECISION_REQUIRED", "promotion_planner_unavailable"
                )
        try:
            verifier = route_driver.CurrentMainEvidenceVerifier(self.repo_path, accepted_main)
            return verifier.verify(output or "", successor, predecessor_receipt)
        except route_driver.RouteDriverError as exc:
            return route_driver.PromotionPlanResult("DECISION_REQUIRED", exc.reason)

    def _dispatch_bounded_pause(
        self, packet_id: str, attempt: str, reason: str
    ) -> local_loop.LocalRunOnceResult:
        try:
            self.github.dispatch_controller(
                "promote-plan", {"packet_id": packet_id, "attempt_id": attempt}
            )
        except local_loop.LoopUnavailable:
            return self._plan_result("bounded_pause", packet_id, attempt, reason=reason)
        return self._plan_result("bounded_pause", packet_id, attempt, reason=reason)

    def _drive_promotion_pr(
        self,
        packet_id: str,
        attempt: str,
        accepted_main: str,
        ledger_issue: int,
        compiled: route_driver.CompiledSuccessor,
        claim_details: dict[str, Any],
    ) -> local_loop.LocalRunOnceResult:
        successor_id = compiled.packet_id
        branch = compiled.branch
        remote_head = self._remote_branch_head(branch)
        if remote_head is not None:
            return self._resume_promotion_pr(
                packet_id, attempt, accepted_main, ledger_issue, compiled, remote_head
            )
        worktree_path: Path | None = None
        try:
            created = worktree_manager.create_plan_worktree(
                successor_id, branch, str(self.repo_path), accepted_main
            )
            if not created:
                return self._plan_result("failed", packet_id, attempt, reason="worktree_failed")
            worktree_path = Path(created[0])
            base_sha, expected_remote_sha = created[2], created[3]
            for relative, content in (
                ("docs/NEXT_DECISION.md", compiled.next_document),
                ("docs/FUTURE_ROUTE.md", compiled.future_document),
                ("docs/CURRENT_STATUS.md", compiled.status_document),
            ):
                target = worktree_path / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(content, encoding="utf-8")
            checks = [
                ["python3", "scripts/check_agent_handoff.py"],
                ["git", "diff", "--check"],
            ]
            for command in checks:
                exit_code, _stdout, _stderr = _bounded_process(
                    command, cwd=str(worktree_path), timeout_seconds=self.command_timeout_seconds
                )
                if exit_code != 0:
                    return self._plan_result("failed", packet_id, attempt, reason="promotion_docs_checks_failed")
            self._git_checked(worktree_path, "diff", "--check")
            self._git_checked(
                worktree_path, "commit", "-am",
                f"docs: compile and promote {successor_id} after closeout of {packet_id}",
            )
            head_sha = self._git_checked(worktree_path, "rev-parse", "HEAD")
            if not local_loop.HEX40.fullmatch(head_sha):
                return self._plan_result("failed", packet_id, attempt, reason="commit_sha_invalid")
            push_args = ["push"]
            if expected_remote_sha:
                push_args.append(f"--force-with-lease=refs/heads/{branch}:{expected_remote_sha}")
            push_args.extend(["origin", f"HEAD:refs/heads/{branch}"])
            push_code, _stdout, _stderr = _bounded_process(
                ["git", *push_args], cwd=str(worktree_path), timeout_seconds=120
            )
            remote = self._git_checked(self.repo_path, "ls-remote", "origin", f"refs/heads/{branch}")
            parts = remote.split()
            remote_head = parts[0] if parts and len(parts) == 2 else None
            if remote_head != head_sha:
                return self._plan_result("failed", packet_id, attempt, reason="push_not_applied")
            if push_code != 0:
                return self._plan_result("failed", packet_id, attempt, reason="push_not_applied")
            marker = json.dumps({
                "subject_kind": "plan-packet", "subject_id": successor_id,
                "source_main_sha": accepted_main,
                "task_spec_sha256": compiled.spec_digest,
                "branch": branch,
            }, sort_keys=True)
            pr = pr_binding.create_or_update_plan_pr(
                successor_id, branch, head_sha, accepted_main,
                compiled.spec_digest, compiled.capsule["goal"][:200],
                f"<!-- agent-orchestrator-binding: {marker} -->\n\n"
                f"Compiled promotion of plan packet `{successor_id}` after "
                f"closeout of `{packet_id}`.",
                self.repository,
            )
            pr_number = pr.get("number")
            if type(pr_number) is not int:
                return self._plan_result("failed", packet_id, attempt, reason="pr_number_unavailable")
            pr_binding.verify_post_push_plan_binding(
                successor_id, pr_number, branch, head_sha,
                accepted_main, compiled.spec_digest, self.repository,
            )
            return self._plan_result(
                "promotion_pr", packet_id, attempt,
                pr_number=pr_number, head_sha=head_sha, branch=branch,
                successor_id=successor_id, accepted_main_sha=accepted_main,
            )
        except (local_loop.LoopUnavailable, pr_binding.PRBindingError, OSError, ValueError) as exc:
            return self._plan_result("failed", packet_id, attempt, reason=str(exc)[:200])
        finally:
            if worktree_path is not None:
                worktree_manager.remove_plan_worktree(successor_id, str(self.repo_path), branch)

    def _remote_branch_head(self, branch: str) -> str | None:
        try:
            remote = self._git_checked(
                self.repo_path, "ls-remote", "origin", f"refs/heads/{branch}"
            )
        except local_loop.LoopUnavailable:
            return None
        parts = remote.split()
        if parts and len(parts) == 2 and local_loop.HEX40.fullmatch(parts[0]):
            return parts[0]
        return None

    def _resume_promotion_pr(
        self,
        packet_id: str,
        attempt: str,
        accepted_main: str,
        ledger_issue: int,
        compiled: route_driver.CompiledSuccessor,
        remote_head: str,
    ) -> local_loop.LocalRunOnceResult:
        """Resume one promotion PR idempotently against its existing remote head.

        The compile is deterministic, so the first drive publishes the branch
        once; every resume verifies the existing exact-head promotion PR
        binding against the remote head and never re-commits or re-opens a
        duplicate PR.  While the PR is a Draft, the Draft-only plan-PR binding
        verifies it; after the driver has marked it Ready (or the merge
        owner merged it), the exact-head plan binding from the authoritative
        merge owner verifies it instead.  Promotion proceeds only after the
        exact-head review receipt, the Ready transition, and the exact-head
        canonical CI success are provable, and the eligible merge is verified
        through the authoritative merge owner.  The canonical ``tests``
        workflow triggers on ``ready_for_review``, so Ready must precede the
        canonical CI gate.
        """

        successor_id = compiled.packet_id
        branch = compiled.branch
        try:
            existing = pr_binding.find_plan_pr(
                successor_id, branch, remote_head, accepted_main,
                compiled.spec_digest, self.repository,
            )
        except pr_binding.PRBindingError:
            existing = self._resolve_non_draft_pr(branch, remote_head, successor_id)
        if existing is None:
            return self._plan_result(
                "promotion_pr", packet_id, attempt,
                branch=branch, successor_id=successor_id,
                reason="promotion_pr_binding_unverified",
            )
        pr_number = existing.get("number")
        head_sha = existing.get("head_sha", "")
        if type(pr_number) is not int or not local_loop.HEX40.fullmatch(head_sha):
            return self._plan_result("failed", packet_id, attempt, reason="pr_binding_invalid")
        review = plan_lifecycle.plan_review_receipt(
            ledger_issue, pr_number, head_sha, self.repository
        )
        if review is None:
            return self._plan_result(
                "promotion_review_pending", packet_id, attempt,
                pr_number=pr_number, head_sha=head_sha, branch=branch,
                successor_id=successor_id, reason="review_receipt_pending",
            )
        if existing.get("isDraft") is True:
            try:
                ready = subprocess.run(
                    ["gh", "pr", "ready", str(pr_number), "--repo", self.repository],
                    capture_output=True, text=True, timeout=self.command_timeout_seconds,
                    check=False,
                )
            except (OSError, subprocess.TimeoutExpired):
                return self._plan_result(
                    "promotion_ready_pending", packet_id, attempt,
                    pr_number=pr_number, head_sha=head_sha, branch=branch,
                    successor_id=successor_id, reason="ready_transition_unavailable",
                )
            if ready.returncode != 0:
                return self._plan_result(
                    "promotion_ready_pending", packet_id, attempt,
                    pr_number=pr_number, head_sha=head_sha, branch=branch,
                    successor_id=successor_id, reason="ready_transition_failed",
                )
        ci_ok = self._exact_head_canonical_ci(pr_number, branch, head_sha)
        if not ci_ok:
            return self._plan_result(
                "promotion_ci_pending", packet_id, attempt,
                pr_number=pr_number, head_sha=head_sha, branch=branch,
                successor_id=successor_id, reason="canonical_ci_pending",
            )
        merge_commit_sha = dispatcher._authoritative_plan_merge(
            pr_number, head_sha, self.repository
        )
        if merge_commit_sha:
            if compiled.packet_state == "T3_REQUIRED" and compiled.t3_request is not None:
                return self._plan_result(
                    "t3_required", packet_id, attempt,
                    successor_id=successor_id, pr_number=pr_number, head_sha=head_sha,
                    merge_commit_sha=merge_commit_sha,
                    t3_request={
                        "packet_id": compiled.t3_request.packet_id,
                        "accepted_main_sha": compiled.t3_request.accepted_main_sha,
                        "candidate_digest": compiled.t3_request.candidate_digest,
                        "action_digest": compiled.t3_request.action_digest,
                        "scope_digest": compiled.t3_request.scope_digest,
                        "requested_action": compiled.t3_request.requested_action,
                    },
                )
            return self._settle_promotion(
                packet_id, attempt, ledger_issue, successor_id,
                pr_number, head_sha, merge_commit_sha,
            )
        return self._plan_result(
            "promotion_pr", packet_id, attempt,
            pr_number=pr_number, head_sha=head_sha, branch=branch,
            successor_id=successor_id,
        )

    def _resolve_non_draft_pr(
        self, branch: str, expected_sha: str, subject_id: str
    ) -> dict[str, Any] | None:
        """Resolve an already-Ready or already-merged promotion PR.

        ``pr_binding.find_plan_pr`` binds only Draft plan PRs; once the
        promotion PR is Ready or merged, the authoritative branch listing and
        the exact-head plan binding from the merge owner provide the same
        fail-closed identity, so a later resume can still verify and settle.
        """

        try:
            result = subprocess.run(
                ["gh", "pr", "list", "--head", branch, "--state", "all",
                 "--repo", self.repository, "--json", "number,headRefOid,isDraft"],
                capture_output=True, text=True, timeout=self.command_timeout_seconds,
                check=False,
            )
            candidates = json.loads(result.stdout) if result.returncode == 0 else []
        except (OSError, ValueError):
            return None
        if not isinstance(candidates, list) or len(candidates) != 1:
            return None
        entry = candidates[0]
        number = entry.get("number")
        head = entry.get("headRefOid")
        if not isinstance(number, int) or head != expected_sha:
            return None
        if not dispatcher._verified_plan_pr(number, expected_sha, subject_id, self.repository):
            return None
        return {"number": number, "head_sha": expected_sha}

    def _exact_head_canonical_ci(self, pr_number: int, branch: str, head_sha: str) -> bool:
        try:
            runs = ci_verifier.find_exact_runs(branch, head_sha, pr_number)
            selected = ci_verifier.select_canonical_run(runs)
        except (local_loop.LoopUnavailable, ValueError):
            return False
        return (
            selected is not None
            and str(selected.get("conclusion", "")).lower() == "success"
            and selected.get("headSha") == head_sha
        )

    def _settle_promotion(
        self,
        packet_id: str,
        attempt: str,
        ledger_issue: int,
        successor_id: str,
        pr_number: int,
        head_sha: str,
        merge_commit_sha: str,
    ) -> local_loop.LocalRunOnceResult:
        try:
            self.github.dispatch_controller(
                "promote-plan", {"packet_id": packet_id, "attempt_id": attempt}
            )
        except local_loop.LoopUnavailable:
            return self._plan_result("promotion_pending", packet_id, attempt, reason="promote_dispatch_unavailable")
        promotion = self._read_plan_promotion(ledger_issue, packet_id, attempt)
        promotion_classification = self._promotion_receipt_classification(promotion)
        if promotion_classification == "escalated":
            return self._promotion_escalation_pause(
                packet_id, attempt, promotion,
                successor_id=successor_id, pr_number=pr_number, head_sha=head_sha,
                merge_commit_sha=merge_commit_sha,
            )
        if promotion_classification == "invalid":
            return self._plan_result(
                "rejected", packet_id, attempt,
                reason="promotion_receipt_invalid", promotion=promotion,
            )
        return self._plan_result(
            "promoted" if promotion_classification == "promoted" else "promotion_pending",
            packet_id, attempt,
            successor_id=successor_id, pr_number=pr_number, head_sha=head_sha,
            merge_commit_sha=merge_commit_sha,
            promotion=promotion or {},
        )

    def _recover_existing_claim(
        self,
        issue: int,
        attempt: str,
        token: str,
        details: dict[str, Any],
    ) -> local_loop.LocalRunOnceResult | None:
        """Repair only a provable exact-head handoff; never recreate output."""

        valid, reason = state_manager.local_claim_binding_valid(issue, details, attempt, token)
        if not valid:
            return self._result("claim_rejected", issue, attempt, reason=reason)
        branch = details.get("canonical_branch")
        if branch != f"agent/issue-{issue}":
            return self._result("claim_rejected", issue, attempt, reason="claim_branch_binding_invalid")
        try:
            remote = self._git_checked(
                self.repo_path, "ls-remote", "origin", f"refs/heads/{branch}"
            )
        except local_loop.LoopUnavailable:
            return self._result("outcome_unknown", issue, attempt, reason="remote_head_unavailable")
        remote_parts = remote.split()
        if remote_parts and (
            len(remote_parts) != 2 or remote_parts[1] != f"refs/heads/{branch}"
        ):
            return self._result("outcome_unknown", issue, attempt, reason="remote_head_ambiguous")
        candidate = remote_parts[0] if remote_parts else ""
        if candidate and not local_loop.HEX40.fullmatch(candidate):
            return self._result("outcome_unknown", issue, attempt, reason="remote_head_invalid")
        if not local_loop.HEX40.fullmatch(candidate) or candidate == details.get("accepted_main_sha"):
            return None
        head_sha = candidate
        try:
            pr = pr_binding.find_issue_pr(issue, branch, head_sha, self.repository)
        except pr_binding.PRBindingError:
            try:
                snapshot = self.github.issue_snapshot(issue)
                pr_body = (
                    f"<!-- agent-orchestrator-binding: {{\"issue_number\":{issue},\"branch\":\"{branch}\"}} -->\n\n"
                    f"Closes #{issue}\n\nLocal run attempt `{attempt}`."
                )
                pr = pr_binding.create_or_update_pr(
                    issue, branch, head_sha, snapshot["title"], pr_body, self.repository
                )
            except (local_loop.LoopUnavailable, pr_binding.PRBindingError, KeyError):
                try:
                    pr = pr_binding.find_issue_pr(issue, branch, head_sha, self.repository)
                except pr_binding.PRBindingError:
                    return self._result("outcome_unknown", issue, attempt, reason="pr_outcome_unknown")
        pr_number = pr.get("number")
        if type(pr_number) is not int:
            return None
        handed_off, handoff_reason = self._request_handoff(
            issue, attempt, token, details["claim_nonce"], pr_number, head_sha
        )
        if not handed_off:
            return self._result("outcome_unknown", issue, attempt, reason=handoff_reason)
        return self._result(
            "handed_off", issue, attempt,
            pr_number=pr_number, head_sha=head_sha, branch=branch,
        )

    def _git_checked(self, worktree: Path, *args: str) -> str:
        try:
            result = subprocess.run(
                ["git", *args], cwd=worktree, capture_output=True, text=True, timeout=120
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise local_loop.LoopUnavailable("local Git command is unavailable") from exc
        if result.returncode != 0:
            raise local_loop.LoopUnavailable("local Git command failed")
        return result.stdout.strip()

    def _owned_artifact_dir(
        self, issue: int, attempt: str, branch: str, base_sha: str, claim_nonce: str
    ) -> Path:
        """Return the per-attempt recovery cache and establish its ownership."""

        directory = self.repo_path.parent / ".agent-control" / "local-runs" / str(issue) / attempt
        artifact_contract.ensure_private_directory(directory)
        receipt = directory / "ownership.json"
        expected = {
            "kind": "repo-agent-local-artifact-ownership.v1",
            "issue_number": issue,
            "attempt_id": attempt,
            "branch": branch,
            "base_sha": base_sha,
            "claim_nonce": claim_nonce,
        }
        if receipt.exists() or receipt.is_symlink():
            artifact_contract._artifact_path_safe(receipt)
            try:
                existing = json.loads(receipt.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError) as exc:
                raise local_loop.LoopUnavailable("local artifact ownership is unreadable") from exc
            if existing != expected:
                raise local_loop.LoopUnavailable("local artifact ownership is conflicting")
        else:
            try:
                artifact_contract.atomic_write_json(receipt, expected)
            except artifact_contract.ArtifactContractError as exc:
                raise local_loop.LoopUnavailable("local artifact ownership is unwritable") from exc
        return directory

    def run_once(self, issue_number: int, attempt_id: str) -> local_loop.LocalRunOnceResult:
        """Run one exact Issue attempt; callers cannot provide derived inputs."""

        attempt = _canonical_attempt_id(attempt_id)
        if type(issue_number) is not int or issue_number <= 0:
            return self._result("rejected", issue_number, str(attempt_id), reason="invalid_issue")
        if attempt is None:
            return self._result("rejected", issue_number, str(attempt_id), reason="invalid_attempt_id")
        token = self._client_token(issue_number, attempt)
        dispatch_id = self._dispatch_id(issue_number, attempt)
        worktree_path: Path | None = None
        branch = f"agent/issue-{issue_number}"
        claimed = False
        pushed = False
        unknown_output = False
        details: dict[str, Any] | None = None
        try:
            control = self.github.read_control_state()
            if control.get("emergency_stop") or not control.get("orchestrator_enabled"):
                return self._result("control_stopped", issue_number, attempt)
            metadata = self.github.repository_metadata()
            if str(metadata.get("name_with_owner", "")).casefold() != self.repository.casefold():
                return self._result("identity_rejected", issue_number, attempt, reason="repository_identity_mismatch")
            default_branch = metadata.get("default_branch")
            if not isinstance(default_branch, str) or not local_loop.BRANCH.fullmatch(default_branch):
                return self._result("unavailable", issue_number, attempt, reason="default_branch_unavailable")
            accepted_main = self.github.accepted_main_sha(default_branch)
            local_main = self.git.origin_main_sha(self.repo_path, default_branch)
            if not local_loop.HEX40.fullmatch(accepted_main) or accepted_main != local_main:
                return self._result("stale_checkout", issue_number, attempt, accepted_main_sha=accepted_main, local_origin_main_sha=local_main)
            try:
                existing_claim = state_manager.read_dispatch_state(
                    issue_number, dispatch_id, self.repository
                )
            except state_manager.StateUnavailableError:
                return self._result("claim_unavailable", issue_number, attempt)
            if isinstance(existing_claim, dict):
                existing_status = existing_claim.get("status")
                if existing_status == "dispatched":
                    recovered = self._recover_existing_claim(
                        issue_number,
                        attempt,
                        token,
                        existing_claim.get("details")
                        if isinstance(existing_claim.get("details"), dict)
                        else {},
                    )
                    if recovered is not None:
                        return recovered
                    return self._result("in_flight", issue_number, attempt, dispatch_id=dispatch_id)
                if existing_status == "claimed":
                    # Claim persisted but dispatch promotion may have crashed.
                    # Re-enter the controller with the same attempt/token so it
                    # can promote claimed→dispatched or leave a terminal state.
                    existing_details = (
                        existing_claim.get("details")
                        if isinstance(existing_claim.get("details"), dict)
                        else {}
                    )
                    valid, reason = state_manager.local_claim_binding_valid(
                        issue_number, existing_details, attempt, token
                    )
                    if not valid:
                        return self._result(
                            "claim_rejected",
                            issue_number,
                            attempt,
                            reason=reason,
                            dispatch_id=dispatch_id,
                        )
                    self.github.dispatch_controller(
                        "claim-local",
                        {
                            "issue": issue_number,
                            "attempt_id": attempt,
                            "client_token": token,
                        },
                    )
                    details = self._wait_for_claim(issue_number, dispatch_id)
                    if details is None:
                        nonce = existing_details.get("claim_nonce")
                        if (
                            isinstance(nonce, str)
                            and state_manager.CLAIM_NONCE_PATTERN.fullmatch(nonce)
                        ):
                            self._release(
                                issue_number,
                                attempt,
                                token,
                                nonce,
                                "local_environment_failure",
                            )
                        return self._result(
                            "failed",
                            issue_number,
                            attempt,
                            reason="claimed_not_dispatched_reconciled",
                            dispatch_id=dispatch_id,
                        )
                    claimed = True
                elif existing_status in {
                    "failed",
                    "rejected",
                    "outcome_unknown",
                    "failed_unknown_output",
                }:
                    return self._result(
                        "terminal", issue_number, attempt,
                        dispatch_id=dispatch_id, claim_status=existing_status,
                    )
            if not claimed:
                self.github.dispatch_controller(
                    "claim-local",
                    {"issue": issue_number, "attempt_id": attempt, "client_token": token},
                )
                details = self._wait_for_claim(issue_number, dispatch_id)
                if details is None:
                    # Claim may have been durable-written without dispatch
                    # promotion.  Supervisor attempts use fresh UUIDs, so this
                    # generation must release itself rather than waiting for a
                    # same-attempt resume that will never arrive.
                    self._reconcile_unproven_claim(
                        issue_number, attempt, token, dispatch_id
                    )
                    return self._result(
                        "claim_unavailable",
                        issue_number,
                        attempt,
                        reason="claim_wait_unproven_reconciled",
                    )
                claimed = True
            if not isinstance(details, dict):
                self._reconcile_unproven_claim(
                    issue_number, attempt, token, dispatch_id
                )
                return self._result("claim_unavailable", issue_number, attempt)
            valid, reason = state_manager.local_claim_binding_valid(
                issue_number, details, attempt, token
            )
            if not valid:
                return self._result("claim_rejected", issue_number, attempt, reason=reason)
            claim_main = details["accepted_main_sha"]
            canonical_branch = details["canonical_branch"]
            if claim_main != accepted_main or canonical_branch != branch:
                return self._result("claim_rejected", issue_number, attempt, reason="claim_identity_mismatch")
            if self.github.accepted_main_sha(default_branch) != claim_main:
                return self._result("claim_rejected", issue_number, attempt, reason="accepted_main_moved")
            if self.git.origin_main_sha(self.repo_path, default_branch) != claim_main:
                return self._result("stale_checkout", issue_number, attempt, accepted_main_sha=claim_main)
            live_control = self.github.read_control_state()
            if live_control.get("emergency_stop") or not live_control.get("orchestrator_enabled"):
                return self._result("control_stopped", issue_number, attempt)
            labels = self.github.labels_for_issue(issue_number)
            if state_manager.LABEL_RUNNING not in labels:
                return self._result("claim_rejected", issue_number, attempt, reason="issue_not_running")
            snapshot = self.github.issue_snapshot(issue_number)
            binding = artifact_contract.build_issue_scope_binding(snapshot["body"])
            if binding != {
                "allowed_paths": details["allowed_paths"],
                "task_body_sha256": details["task_body_sha256"],
            }:
                return self._result("claim_rejected", issue_number, attempt, reason="task_body_changed")
            created = worktree_manager.create_worktree(
                issue_number, branch, str(self.repo_path), claim_main
            )
            if not created:
                return self._result("failed", issue_number, attempt, reason="worktree_failed")
            worktree_path = Path(created[0])
            base_sha, expected_remote_sha = created[2], created[3]
            if base_sha != claim_main:
                return self._result("failed", issue_number, attempt, reason="worktree_base_mismatch")
            with tempfile.TemporaryDirectory(prefix=f"agent-run-{issue_number}-") as temp:
                temp_dir = Path(temp)
                artifact_dir = self._owned_artifact_dir(
                    issue_number, attempt, branch, base_sha, details["claim_nonce"]
                )
                prompt_file = temp_dir / "implementation-prompt.txt"
                prompt_file.write_text(
                    prompt_builder.build_claim_bound_implementation_prompt(
                        issue_number, snapshot["title"], snapshot["body"], details["allowed_paths"],
                        claim_main, branch, repo_root=self.repo_path,
                    ),
                    encoding="utf-8",
                )
                output_dir = temp_dir / "codex-output"
                wrapper = Path(__file__).resolve().parent / "codex_wrapper.sh"
                exit_code, _stdout, _stderr = _bounded_process(
                    ["bash", str(wrapper), "implement", str(prompt_file), str(output_dir), str(worktree_path)],
                    timeout_seconds=self.command_timeout_seconds,
                )
                if exit_code != 0:
                    return self._result("failed", issue_number, attempt, reason="codex_failed")
                exit_file = output_dir / "codex-exit-code.txt"
                if not exit_file.is_file() or exit_file.read_text().strip() != "0":
                    return self._result("failed", issue_number, attempt, reason="codex_result_invalid")
                try:
                    local_checks = local_verification.run_issue_focused_checks(worktree_path)
                except local_verification.LocalVerificationError as exc:
                    return self._result(
                        "failed", issue_number, attempt, reason=str(exc.reason)[:200]
                    )
                manifest = artifact_contract.create_artifact(
                    repo=worktree_path, artifact_dir=artifact_dir, worker_type="implementation",
                    issue_number=issue_number, pr_number=0, base_sha=base_sha,
                    expected_remote_sha=expected_remote_sha, branch=branch, codex_exit_code=0,
                    local_checks=local_checks,
                )
                artifact_contract.validate_artifact(
                    artifact_dir=artifact_dir, expected_worker_type="implementation",
                    issue_number=issue_number, pr_number=0, base_sha=base_sha,
                    expected_remote_sha=expected_remote_sha, branch=branch,
                )
                artifact_contract.validate_scope_binding(details, manifest)
                self._git_checked(worktree_path, "reset", "--hard", base_sha)
                self._git_checked(worktree_path, "clean", "-fd")
                self._git_checked(worktree_path, "apply", "--index", "--binary", str(artifact_dir / artifact_contract.PATCH_NAME))
                artifact_contract.validate_index(worktree_path, manifest)
                self._git_checked(worktree_path, "diff", "--check")
                self._git_checked(worktree_path, "commit", "-m", f"feat: implement issue #{issue_number}")
                head_sha = self._git_checked(worktree_path, "rev-parse", "HEAD")
                if not local_loop.HEX40.fullmatch(head_sha):
                    return self._result("failed", issue_number, attempt, reason="commit_sha_invalid")
                push_args = ["push"]
                if expected_remote_sha:
                    push_args.append(f"--force-with-lease=refs/heads/{branch}:{expected_remote_sha}")
                push_args.extend(["origin", f"HEAD:refs/heads/{branch}"])
                push_code, _push_stdout, _push_stderr = _bounded_process(
                    ["git", *push_args], cwd=worktree_path, timeout_seconds=120
                )
                try:
                    remote = self._git_checked(
                        self.repo_path, "ls-remote", "origin", f"refs/heads/{branch}"
                    )
                except local_loop.LoopUnavailable:
                    unknown_output = True
                    return self._unknown_output(issue_number, attempt, token, "remote_head_unavailable")
                remote_parts = remote.split()
                if remote_parts and (
                    len(remote_parts) != 2 or remote_parts[1] != f"refs/heads/{branch}"
                ):
                    unknown_output = True
                    return self._unknown_output(issue_number, attempt, token, "remote_head_ambiguous")
                remote_head = remote_parts[0] if remote_parts else None
                if remote_head and not local_loop.HEX40.fullmatch(remote_head):
                    unknown_output = True
                    return self._unknown_output(issue_number, attempt, token, "remote_head_invalid")
                if remote_head == head_sha:
                    pushed = True
                elif remote_head == expected_remote_sha or (
                    remote_head is None and expected_remote_sha is None
                ):
                    return self._result("failed", issue_number, attempt, reason="push_not_applied")
                else:
                    unknown_output = True
                    return self._unknown_output(issue_number, attempt, token, "push_outcome_unknown")
                if push_code != 0 and not pushed:
                    return self._result("failed", issue_number, attempt, reason="push_not_applied")
                pr_body = (
                    f"<!-- agent-orchestrator-binding: {{\"issue_number\":{issue_number},\"branch\":\"{branch}\"}} -->\n\n"
                    f"Closes #{issue_number}\n\nLocal run attempt `{attempt}`."
                )
                pr = pr_binding.create_or_update_pr(
                    issue_number, branch, head_sha, snapshot["title"], pr_body, self.repository
                )
                pr_number = pr.get("number")
                if type(pr_number) is not int:
                    unknown_output = True
                    return self._unknown_output(issue_number, attempt, token, "pr_number_unavailable")
                pr_binding.verify_post_push_binding(
                    issue_number, pr_number, branch, head_sha, self.repository
                )
                handed_off, handoff_reason = self._request_handoff(
                    issue_number, attempt, token, details["claim_nonce"], pr_number, head_sha
                )
                if not handed_off:
                    unknown_output = True
                    return self._unknown_output(issue_number, attempt, token, handoff_reason)
                return self._result(
                    "handed_off", issue_number, attempt,
                    pr_number=pr_number, head_sha=head_sha, branch=branch,
                    accepted_main_sha=claim_main, ci_monitor="controller-handoff",
                )
        except (local_loop.LoopUnavailable, artifact_contract.ArtifactContractError, pr_binding.PRBindingError, OSError, ValueError) as exc:
            if pushed:
                unknown_output = True
                return self._unknown_output(issue_number, attempt, token, "external_outcome_unknown")
            return self._result("failed", issue_number, attempt, reason=str(exc)[:200])
        finally:
            if worktree_path is not None:
                worktree_manager.remove_worktree(issue_number, str(self.repo_path), branch)
            if claimed and not pushed and not unknown_output:
                try:
                    nonce = self._claim_nonce(issue_number, attempt)
                except local_loop.LoopUnavailable:
                    nonce = ""
                if nonce:
                    self._release(
                        issue_number,
                        attempt,
                        token,
                        nonce,
                        "local_environment_failure",
                    )
