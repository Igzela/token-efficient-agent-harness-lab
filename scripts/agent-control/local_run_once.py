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
import local_loop
import local_verification
import plan_lane
import pr_binding
import prompt_builder
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


def _bounded_process(
    command: list[str], *, cwd: Path | None = None, timeout_seconds: int = 1800
) -> tuple[int, str, str]:
    """Run one child in an isolated session with tree-scoped cancellation.

    The child is started with ``start_new_session=True`` so its process group
    is never the run-once/receipt owner.  Timeouts terminate only the child
    PID tree; the caller survives to emit a truthful non-success receipt.
    """

    try:
        process = subprocess.Popen(
            command,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            start_new_session=True,
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
        sleeper: Callable[[float], None] = time.sleep,
    ) -> None:
        if not local_loop.REPOSITORY.fullmatch(repository):
            raise ValueError("repository must be owner/name")
        if claim_timeout_seconds < 0 or claim_timeout_seconds > 900:
            raise ValueError("claim_timeout_seconds is outside the bounded range")
        if command_timeout_seconds < 1 or command_timeout_seconds > 3600:
            raise ValueError("command_timeout_seconds is outside the bounded range")
        if poll_interval_seconds < 0 or poll_interval_seconds > 30:
            raise ValueError("poll_interval_seconds is outside the bounded range")
        self.github = github or local_loop.GitHubAdapter(repository)
        self.git = git or local_loop.GitAdapter()
        self.repository = repository
        self.repo_path = Path(repo_path).expanduser().resolve()
        self.claim_timeout_seconds = claim_timeout_seconds
        self.command_timeout_seconds = command_timeout_seconds
        self.poll_interval_seconds = poll_interval_seconds
        self.sleeper = sleeper

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
        """Plan execution is deferred until CI/review/terminal owners are plan-aware.

        The independent review established that Plan Draft PRs cannot enter the
        existing Issue-bound CI/monitor lifecycle, and accepted main does not
        authorize expanding this packet with a parallel Plan terminal owner.
        Keep the parser and tests for a later authorized packet; fail closed
        here so capacity cannot be claimed by an unclosable lane.
        """

        attempt = _canonical_attempt_id(attempt_id)
        return self._plan_result(
            "rejected",
            str(packet_id),
            attempt if attempt is not None else str(attempt_id),
            reason="plan_lane_deferred_until_terminal_owners",
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
                        candidate.source_main_sha, candidate.branch, repo_root=self.repo_path,
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
                return self._plan_result(
                    "handed_off", packet_id, attempt, pr_number=pr_number,
                    head_sha=head_sha, branch=candidate.branch, accepted_main_sha=accepted_main,
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
                    return self._result("claim_unavailable", issue_number, attempt)
                claimed = True
            if not isinstance(details, dict):
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
