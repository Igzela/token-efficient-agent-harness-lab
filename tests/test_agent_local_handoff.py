"""Provider-free tests for the trusted local handoff/release gateway.

``dispatcher.handoff_local`` is the server-side handoff gate: a local process
supplies only Issue, canonical attempt id, and client token, and the server
revalidates the newest exact trusted local-run claim binding plus live Issue
body/label/accepted-main/PR state before idempotently persisting worker
state, reusing or acquiring exact-head CI, and dispatching the trusted
agent-ci-monitor under a durable receipt.  ``dispatcher.release_local``
terminalizes a known pre-handoff failure while preserving the entire claim
binding.  Every test runs without any provider call.
"""

from __future__ import annotations

import contextlib
import hashlib
import json
import pathlib
import re
import sys
import unittest
from contextlib import redirect_stdout
from datetime import datetime, timedelta, timezone
from io import StringIO
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[1]
CONTROL = ROOT / "scripts" / "agent-control"
WORKFLOWS = ROOT / ".github" / "workflows"
sys.path.insert(0, str(CONTROL))

import ci_verifier  # noqa: E402
import control_state  # noqa: E402
import dispatcher  # noqa: E402
import local_loop  # noqa: E402
import pr_binding  # noqa: E402
import state_manager as sm  # noqa: E402


MAIN_SHA = "a" * 40
OWNER = "acme"
REPO = f"{OWNER}/repo"
ATTEMPT = "123e4567-e89b-12d3-a456-426614174000"
CLIENT_TOKEN = "c" * 32
NONCE = "d" * 32
# A distinct canonical attempt id / token / nonce for a newer local claim
# generation B that must never be released by a stale terminal retry of A.
ATTEMPT_B = "00000000-0000-0000-0000-000000000001"
TOKEN_B = "e" * 32
NONCE_B = "f" * 32
SCOPE = '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["src/"]} -->'
TASK = f'<!-- repo-agent-task:v1 {{"accepted_main_sha":"{MAIN_SHA}"}} -->'
BODY = f"{SCOPE}\n{TASK}"
ISSUE = 77
PR = 123
HEAD = "b" * 40
RUN = 987
CANONICAL_BRANCH = f"agent/issue-{ISSUE}"
DIGEST = hashlib.sha256(BODY.encode("utf-8")).hexdigest()
DISPATCH_ID = f"local-run:{ISSUE}:{ATTEMPT}"
DISPATCH_ID_B = f"local-run:{ISSUE}:{ATTEMPT_B}"
RECEIPT_ID = f"ci-monitor:{PR}:{HEAD}:{RUN}"
REASON = "local_worktree_failure"
FUTURE_LEASE = (
    datetime.now(timezone.utc) + timedelta(hours=2)
).isoformat().replace("+00:00", "Z")


def state(issue, dispatch_id, action, status, details, *, kind="agent-orchestrator-dispatch-state", version=1):
    return {
        "kind": kind,
        "version": version,
        "issue_number": issue,
        "dispatch_id": dispatch_id,
        "action": action,
        "status": status,
        "details": dict(details),
    }


def trusted_comment(state, author="github-actions[bot]"):
    """Wrap a persisted state as an Issue-comment object (outer order preserved).

    ``get_issue_comments`` returns comments newest first, so callers pass a
    list of ``trusted_comment`` items in the exact order ``release_local`` /
    ``read_exact_ci_state`` must observe them.
    """

    return {"author": {"login": author}, "body": json.dumps(state, sort_keys=True)}


def claim_details(**overrides):
    details = {
        "issue_number": ISSUE,
        "attempt_id": ATTEMPT,
        "client_token": CLIENT_TOKEN,
        "accepted_main_sha": MAIN_SHA,
        "canonical_branch": CANONICAL_BRANCH,
        "lease_deadline": FUTURE_LEASE,
        "previous_labels": [sm.LABEL_READY],
        "target_label": sm.LABEL_RUNNING,
        "allowed_paths": ["src/"],
        "task_body_sha256": DIGEST,
        "claim_nonce": NONCE,
        **overrides,
    }
    return details


def claimed_state(status="dispatched", *, action="local-run", version=1, kind="agent-orchestrator-dispatch-state", dispatch_id=DISPATCH_ID, issue_number=ISSUE, **overrides):
    return state(issue_number, dispatch_id, action, status, claim_details(**overrides), kind=kind, version=version)


def monitor_receipt(pr=PR, head=HEAD, run=RUN, status="dispatched", **overrides):
    details = {
        "issue_number": ISSUE,
        "pr_number": pr,
        "head_sha": head,
        "ci_run_id": run,
        "workflow": dispatcher.MONITOR_WORKFLOW,
        **overrides,
    }
    return state(ISSUE, f"ci-monitor:{pr}:{head}:{run}", "ci-monitor", status, details)


def ci_state_record(pr=PR, head=HEAD, run=RUN, status="dispatched"):
    return {
        "kind": "agent-orchestrator-ci-state",
        "version": 2,
        "pr_number": pr,
        "head_sha": head,
        "workflow_run_id": run,
        "workflow_name": "tests",
        "required_jobs": [],
        "successful_jobs": [],
        "status": status,
        "extra": {},
    }


def worker_state(pr=PR, head=HEAD, **extra_overrides):
    extra = {
        "branch": CANONICAL_BRANCH,
        "attempt_id": ATTEMPT,
        "dispatch_id": DISPATCH_ID,
        "claim_nonce": NONCE,
        **extra_overrides,
    }
    return {
        "kind": "agent-orchestrator-state",
        "version": 1,
        "pr_number": pr,
        "head_sha": head,
        "worker_type": "local-run",
        "extra": extra,
    }


def monitor_workflow_input_keys():
    """Return the exact ``workflow_dispatch`` input keys declared by agent-ci-monitor.yml."""
    source = (WORKFLOWS / "agent-ci-monitor.yml").read_text()
    block = source.split("workflow_dispatch:", 1)[1].split("permissions:", 1)[0]
    return [line.strip().rstrip(":") for line in block.splitlines() if re.match(r"^ {6}[a-z_]+:$", line)]


def candidate_pr(pr=PR, head=HEAD, **overrides):
    value = {
        "number": pr,
        "state": "OPEN",
        "baseRefName": "main",
        "headRefName": CANONICAL_BRANCH,
        "headRefOid": head,
        "body": f"Closes #{ISSUE}\n<!-- agent-orchestrator-binding: {{\"issue_number\": {ISSUE}, \"branch\": \"{CANONICAL_BRANCH}\"}} -->",
        "isDraft": True,
        "headRepository": {"nameWithOwner": REPO},
        **overrides,
    }
    return value


class FakeAdapter:
    def __init__(self, metadata=None, main_sha=MAIN_SHA, metadata_error=None, sha_error=None):
        self.metadata = metadata or {
            "name_with_owner": REPO, "owner": OWNER, "is_private": False, "default_branch": "main",
        }
        self.main_sha = main_sha
        self.metadata_error = metadata_error
        self.sha_error = sha_error

    def repository_metadata(self):
        if self.metadata_error is not None:
            raise self.metadata_error
        return dict(self.metadata)

    def accepted_main_sha(self, branch):
        if self.sha_error is not None:
            raise self.sha_error
        return self.main_sha


class LocalHandoffBase(unittest.TestCase):
    def setUp(self):
        self.persisted = {}
        self.records = []
        self.mutation_order = []
        self.label_calls = []
        self.body_reads = 0
        self.worker_records = []
        self.ci_acquisitions = []
        self.ci_states = []
        self.workflow_calls = []
        self.workflow_attempts = []

    def seed_claim(self, claim=None):
        claim = claim or claimed_state()
        self.persisted[claim["dispatch_id"]] = claim
        return claim

    @contextlib.contextmanager
    def command_context(self, **overrides):
        """Enter the full mock set shared by handoff-local and release-local.

        ``read_error`` makes the trusted dispatch-state read fail closed;
        ``record_fail_dispatch`` makes the durable dispatch-state write fail.
        """
        repo_value = overrides.get("repo_value", REPO)
        live = overrides.get("live", True)
        metadata = overrides.get("metadata")
        main_sha = overrides.get("main_sha", MAIN_SHA)
        metadata_error = overrides.get("metadata_error")
        sha_error = overrides.get("sha_error")
        labels = overrides.get("labels", frozenset({sm.LABEL_RUNNING}))
        body = overrides.get("body", BODY)
        worker = overrides.get("worker")
        worker_write_ok = overrides.get("worker_write_ok", True)
        acquisition = overrides.get("acquisition")
        acquisition_write_ok = overrides.get("acquisition_write_ok", True)
        ci_write_ok = overrides.get("ci_write_ok", True)
        ci_read_outcome = overrides.get("ci_read_outcome")
        ci_state = overrides.get("ci_state")
        ci_read_error = overrides.get("ci_read_error")
        acquire_result = overrides.get("acquire_result")
        acquire_error = overrides.get("acquire_error")
        pr_candidate = overrides.get("pr_candidate", candidate_pr())
        find_error = overrides.get("find_error")
        run_workflow_ok = overrides.get("run_workflow_ok", True)
        record_fail_dispatch = overrides.get("record_fail_dispatch", False)
        record_fail_statuses = overrides.get("record_fail_statuses", frozenset())
        read_error = overrides.get("read_error")
        set_labels_ok = overrides.get("set_labels_ok", True)
        comments = overrides.get("comments")
        comments_error = overrides.get("comments_error")

        def read_state(_issue, dispatch_id, _repo=""):
            if read_error is not None:
                raise read_error
            return self.persisted.get(dispatch_id)

        def read_issue_comments(_issue, _repo=""):
            # ``get_issue_comments`` returns comments newest first; an explicit
            # ``comments`` override drives the generation-guard tests, and the
            # default derives from the seeded persisted claims.
            if comments_error is not None:
                raise comments_error
            if comments is not None:
                return [dict(comment) for comment in comments]
            return [trusted_comment(state) for state in self.persisted.values()]

        def record_state(_issue, dispatch_id, action, status, details=None, _repo=""):
            if record_fail_dispatch or status in record_fail_statuses:
                return False
            written = state(_issue, dispatch_id, action, status, details)
            self.records.append(written)
            self.persisted[dispatch_id] = written
            self.mutation_order.append(("record-dispatch", action, status))
            return True

        def counting_body(_issue, _repo=""):
            self.body_reads += 1
            return body

        def read_worker(_issue, _repo=""):
            return worker

        def record_worker(_issue, pr_number, head_sha, worker_type, extra=None, repo=""):
            if not worker_write_ok:
                return False
            written = {
                "kind": "agent-orchestrator-state",
                "version": 1,
                "pr_number": int(pr_number),
                "head_sha": head_sha,
                "worker_type": worker_type,
                "extra": dict(extra or {}),
            }
            self.worker_records.append(written)
            self.mutation_order.append(("record-worker", worker_type))
            return True

        def read_acquisition(_issue, pr_number=None, head_sha=None, _repo=""):
            if acquisition is None:
                return None
            if pr_number is not None and acquisition.get("pr_number") != int(pr_number):
                return None
            if head_sha is not None and acquisition.get("head_sha") != head_sha:
                return None
            return dict(acquisition)

        def record_acquisition(_issue, pr_number, head_sha, run_id, source,
                               duplicate_run_ids=None, repo="", metadata=None):
            [int(value) for value in (duplicate_run_ids or [])]
            if not acquisition_write_ok:
                return False
            self.ci_acquisitions.append(
                {"pr_number": int(pr_number), "head_sha": head_sha, "run_id": int(run_id), "source": source}
            )
            self.mutation_order.append(("record-ci-acquisition", source))
            return True

        def record_ci(_issue, pr_number, head_sha, run_id, status, extra=None, repo=""):
            if not ci_write_ok:
                return False
            self.ci_states.append(
                {"pr_number": int(pr_number), "head_sha": head_sha, "run_id": int(run_id), "status": status}
            )
            self.mutation_order.append(("record-ci", status))
            return True

        def read_exact_ci(_issue, pr_number, head_sha, ci_run_id, _repo=""):
            if ci_read_error is not None:
                return "unverifiable", str(ci_read_error)
            if ci_read_outcome is not None:
                return ci_read_outcome, ci_state
            return "absent", None

        def set_labels_mock(_issue, *new_labels, repo=""):
            if not set_labels_ok:
                return False
            self.label_calls.append(list(new_labels))
            self.mutation_order.append(("set-labels", tuple(new_labels)))
            return True

        def workflow_mock(workflow, fields):
            self.workflow_attempts.append((workflow, dict(fields)))
            if not run_workflow_ok:
                return False
            self.workflow_calls.append((workflow, dict(fields)))
            self.mutation_order.append(("run-workflow", workflow))
            return True

        def find_pr(_issue, _branch, _expected_sha, _repo):
            if find_error is not None:
                raise find_error
            return dict(pr_candidate)

        def acquire(*_args, **_kwargs):
            if acquire_error is not None:
                raise acquire_error
            if acquire_result is not None:
                return dict(acquire_result)
            return {
                "workflow_run_id": RUN,
                "source": "workflow_dispatch",
                "duplicate_run_ids": [],
                "observed_run_ids": [RUN],
                "selection_reason": "fallback_active_observed",
                "superseded_run_ids": [],
                "unsupported_run_ids": [],
                "fallback_dispatched": True,
                "bound_status": "queued",
            }

        with contextlib.ExitStack() as stack:
            stack.enter_context(mock.patch.object(dispatcher, "_repo", return_value=repo_value))
            stack.enter_context(mock.patch.object(
                dispatcher.control_state, "require_live",
                **({"return_value": {}} if live else {"side_effect": control_state.ControlStateError("stopped")}),
            ))
            fake = FakeAdapter(metadata=metadata, main_sha=main_sha,
                               metadata_error=metadata_error, sha_error=sha_error)
            stack.enter_context(mock.patch.object(dispatcher.local_loop, "GitHubAdapter", return_value=fake))
            stack.enter_context(mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=read_state))
            stack.enter_context(mock.patch.object(dispatcher.sm, "record_dispatch_state", side_effect=record_state))
            stack.enter_context(mock.patch.object(dispatcher.sm, "get_issue_comments", side_effect=read_issue_comments))
            stack.enter_context(mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value=labels))
            stack.enter_context(mock.patch.object(dispatcher.sm, "get_issue_body", side_effect=counting_body))
            stack.enter_context(mock.patch.object(dispatcher.sm, "read_worker_state", side_effect=read_worker))
            stack.enter_context(mock.patch.object(dispatcher.sm, "record_worker_state", side_effect=record_worker))
            stack.enter_context(mock.patch.object(dispatcher.sm, "read_ci_acquisition", side_effect=read_acquisition))
            stack.enter_context(mock.patch.object(dispatcher.sm, "record_ci_acquisition", side_effect=record_acquisition))
            stack.enter_context(mock.patch.object(dispatcher.sm, "record_ci_state", side_effect=record_ci))
            stack.enter_context(mock.patch.object(dispatcher.sm, "read_exact_ci_state", side_effect=read_exact_ci))
            stack.enter_context(mock.patch.object(dispatcher.sm, "set_labels", side_effect=set_labels_mock))
            stack.enter_context(mock.patch.object(dispatcher, "_run_workflow", side_effect=workflow_mock))
            stack.enter_context(mock.patch.object(dispatcher.pr_binding, "find_issue_pr", side_effect=find_pr))
            stack.enter_context(mock.patch.object(dispatcher.ci_verifier, "acquire_exact_run", side_effect=acquire))
            gh = stack.enter_context(mock.patch.object(dispatcher.sm, "_gh"))
            yield {"gh": gh}


class TestInputValidation(LocalHandoffBase):
    def test_handoff_rejects_bad_attempt_and_token(self):
        for bad in ("not-a-uuid", ATTEMPT.upper(), ATTEMPT.replace("-", ""), ""):
            with self.subTest(attempt=bad):
                self.assertEqual(dispatcher.handoff_local(ISSUE, bad, CLIENT_TOKEN, HEAD),
                                 {"handed_off": False, "issue": ISSUE, "reason": "invalid_attempt_id"})
        for bad in ("z" * 32, "C" * 32, "c" * 31, ""):
            with self.subTest(token=bad):
                self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, bad, HEAD),
                                 {"handed_off": False, "issue": ISSUE, "reason": "invalid_client_token"})
        for bad in ("", "b" * 39, "g" * 40, "B" * 39 + "z", "not-a-sha"):
            with self.subTest(head=bad):
                self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, bad),
                                 {"handed_off": False, "issue": ISSUE, "reason": "invalid_head_sha"})

    def test_release_rejects_bad_inputs(self):
        for bad in ("not-a-uuid", "z" * 32):
            with self.subTest(attempt=bad):
                self.assertEqual(dispatcher.release_local(ISSUE, bad, CLIENT_TOKEN, REASON)["reason"],
                                 "invalid_attempt_id")
        for bad in ("z" * 32, "C" * 32):
            with self.subTest(token=bad):
                self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, bad, REASON)["reason"],
                                 "invalid_client_token")
        for bad in ("", "free text", "local_worktree_failure ", "LOCAL_ABORTED", "x" * 65):
            with self.subTest(reason=bad):
                self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, bad)["reason"],
                                 "invalid_reason_code")


class TestControlAndRepositoryNegatives(LocalHandoffBase):
    def test_handoff_control_and_repository_negatives_fail_closed(self):
        with self.command_context(live=False):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"],
                             "disabled_or_emergency_stopped")
        with self.command_context(repo_value=""):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"],
                             "repository_unavailable")
        with self.command_context(metadata_error=local_loop.LoopUnavailable("boom")):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"],
                             "repository_state_unavailable")
        with self.command_context(metadata={"name_with_owner": REPO}):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"],
                             "default_branch_unavailable")
        for branch in ("bad branch!", "x" * 201, ""):
            with self.subTest(branch=branch):
                with self.command_context(metadata={"name_with_owner": REPO, "owner": OWNER, "default_branch": branch}):
                    self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"],
                                     "default_branch_unavailable")
        with self.command_context(sha_error=local_loop.LoopUnavailable("boom")):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"],
                             "repository_state_unavailable")
        with self.command_context(main_sha="not-a-sha"):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"],
                             "accepted_main_unavailable")

    def test_release_control_and_repository_negatives_fail_closed(self):
        with self.command_context(live=False):
            self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)["reason"],
                             "disabled_or_emergency_stopped")
        with self.command_context(repo_value=""):
            self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)["reason"],
                             "repository_unavailable")


class TestClaimStateNegatives(LocalHandoffBase):
    def test_handoff_requires_exact_dispatched_claim(self):
        for status, expected in (
            (None, "claim_not_found"),
            ("claimed", "dispatch_in_flight"),
            ("failed", "claim_state_unexpected"),
            ("rollback", "claim_state_unexpected"),
        ):
            with self.subTest(status=status):
                if status is not None:
                    self.seed_claim(claimed_state(status=status))
                with self.command_context() as mocks:
                    result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
                self.assertEqual(result["reason"], expected)
                mocks["gh"].assert_not_called()

    def test_handoff_wrong_action_attempt_or_token_fails_closed(self):
        self.seed_claim(claimed_state(action="worker"))
        with self.command_context():
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "claim_action_mismatch")
        self.persisted.clear()
        self.seed_claim(claimed_state(attempt_id="0" * 36))
        with self.command_context():
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "claim_attempt_mismatch")
        self.persisted.clear()
        self.seed_claim(claimed_state(client_token="e" * 32))
        with self.command_context():
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "claim_token_mismatch")

    def test_handoff_unreadable_state_fails_closed(self):
        with self.command_context(read_error=sm.StateUnavailableError("malformed")):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"],
                             "dispatch_state_unavailable")
        self.persisted.clear()
        self.seed_claim(claimed_state(version=2))
        with self.command_context(read_error=sm.StateUnavailableError("version")):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"],
                             "dispatch_state_unavailable")

    def test_handoff_requires_exact_claim_identity(self):
        self.seed_claim(claimed_state(version=2))
        with self.command_context() as mocks:
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "claim_malformed")
            mocks["gh"].assert_not_called()
        self.persisted.clear()
        self.seed_claim(claimed_state(kind="agent-orchestrator-worker-state"))
        with self.command_context() as mocks:
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "claim_malformed")
            mocks["gh"].assert_not_called()
        self.persisted.clear()
        self.seed_claim(claimed_state(issue_number=ISSUE + 1))
        with self.command_context() as mocks:
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "claim_malformed")
            mocks["gh"].assert_not_called()
        self.persisted.clear()
        self.persisted[DISPATCH_ID] = claimed_state(
            dispatch_id="local-run:99:00000000-0000-0000-0000-000000000000"
        )
        with self.command_context() as mocks:
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "claim_malformed")
            mocks["gh"].assert_not_called()

    def test_handoff_rejects_invalid_or_expired_bindings(self):
        for override, expected in (
            ({"accepted_main_sha": "not-hex"}, "claim_main_binding_invalid"),
            ({"accepted_main_sha": "B" * 40}, "claim_main_binding_invalid"),
            ({"canonical_branch": "agent/issue-99"}, "claim_branch_binding_invalid"),
            ({"allowed_paths": []}, "claim_scope_binding_invalid"),
            ({"task_body_sha256": "nope"}, "claim_scope_binding_invalid"),
            ({"claim_nonce": "z" * 32}, "claim_nonce_invalid"),
            ({"lease_deadline": "not-a-date"}, "claim_lease_invalid"),
            ({"lease_deadline": "2020-01-01T00:00:00Z"}, "claim_lease_expired"),
        ):
            with self.subTest(override=override):
                self.persisted.clear()
                self.seed_claim(claimed_state(**override))
                with self.command_context():
                    self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], expected)

    def test_release_requires_exact_claim_identity(self):
        self.seed_claim(claimed_state(version=2))
        with self.command_context() as mocks:
            self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)["reason"],
                             "claim_malformed")
            mocks["gh"].assert_not_called()
        self.persisted.clear()
        self.seed_claim(claimed_state(kind="agent-orchestrator-worker-state"))
        with self.command_context() as mocks:
            self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)["reason"],
                             "claim_malformed")
            mocks["gh"].assert_not_called()
        self.persisted.clear()
        self.seed_claim(claimed_state(issue_number=ISSUE + 1))
        with self.command_context() as mocks:
            self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)["reason"],
                             "claim_malformed")
            mocks["gh"].assert_not_called()
        self.persisted.clear()
        self.persisted[DISPATCH_ID] = claimed_state(
            dispatch_id="local-run:99:00000000-0000-0000-0000-000000000000"
        )
        with self.command_context() as mocks:
            self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)["reason"],
                             "claim_malformed")
            mocks["gh"].assert_not_called()

    def test_release_rejects_unreadable_wrong_or_superseded_claims(self):
        with self.command_context(read_error=sm.StateUnavailableError("malformed")):
            self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)["reason"],
                             "dispatch_state_unavailable")
        self.persisted.clear()
        with self.command_context():
            self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)["reason"],
                             "claim_not_found")
        self.persisted.clear()
        self.seed_claim(claimed_state(status="claimed", action="worker"))
        with self.command_context():
            self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)["reason"],
                             "claim_action_mismatch")
        for status in ("rollback", "rejected"):
            with self.subTest(status=status):
                self.persisted.clear()
                self.seed_claim(claimed_state(status=status))
                with self.command_context():
                    self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)["reason"],
                                     "claim_state_unexpected")
        self.persisted.clear()
        self.seed_claim(claimed_state(status="dispatched", client_token="e" * 32))
        with self.command_context():
            self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)["reason"],
                             "claim_token_mismatch")
        self.persisted.clear()
        self.seed_claim(claimed_state(status="dispatched", lease_deadline="2020-01-01T00:00:00Z"))
        with self.command_context():
            self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)["reason"],
                             "claim_lease_expired")


class TestHandoffLiveRevalidation(LocalHandoffBase):
    def test_handoff_rejects_label_body_main_and_pr_mismatches_before_any_write(self):
        self.seed_claim()
        with self.command_context(labels=None) as mocks:
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "label_state_unavailable")
            self.assert_no_mutation(mocks)
        with self.command_context(labels=frozenset({sm.LABEL_READY})) as mocks:
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "issue_not_running")
            self.assert_no_mutation(mocks)
        with self.command_context(body=None) as mocks:
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "task_body_unavailable")
            self.assert_no_mutation(mocks)
        with self.command_context(body=BODY + "\nextra") as mocks:
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "task_body_changed")
            self.assert_no_mutation(mocks)
        other_scope = BODY.replace("src/", "src/other/")
        self.persisted.clear()
        self.seed_claim(claimed_state(task_body_sha256=hashlib.sha256(other_scope.encode("utf-8")).hexdigest()))
        with self.command_context(body=other_scope) as mocks:
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "scope_changed")
            self.assert_no_mutation(mocks)
        with self.command_context(main_sha="f" * 40) as mocks:
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "accepted_main_moved")
            self.assert_no_mutation(mocks)

    def assert_no_mutation(self, mocks):
        self.assertEqual(self.worker_records, [])
        self.assertEqual(self.ci_acquisitions, [])
        self.assertEqual(self.ci_states, [])
        self.assertEqual(self.workflow_calls, [])
        self.assertEqual(self.label_calls, [])
        mocks["gh"].assert_not_called()

    def test_handoff_rejects_pr_negatives_before_any_write(self):
        self.seed_claim()
        with self.command_context(find_error=pr_binding.PRBindingError("zero or multiple open PRs bound to the Issue branch")) as mocks:
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertTrue(result["reason"].startswith("pr_binding_rejected:"))
        self.assert_no_mutation(mocks)
        with self.command_context(pr_candidate=candidate_pr(baseRefName="develop")) as mocks:
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "pr_base_mismatch")
            self.assert_no_mutation(mocks)
        with self.command_context(pr_candidate=candidate_pr(headRefOid="z" * 40)) as mocks:
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "pr_head_unavailable")
            self.assert_no_mutation(mocks)
        with self.command_context(pr_candidate=candidate_pr(headRefOid="0" * 40)) as mocks:
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "pr_head_mismatch")
            self.assert_no_mutation(mocks)
        with self.command_context(pr_candidate=candidate_pr(number="123")) as mocks:
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertTrue(result["reason"].startswith("pr_binding_rejected:"))
        self.assert_no_mutation(mocks)
        with self.command_context(pr_candidate=candidate_pr(isDraft=False)) as mocks:
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertEqual(result["reason"], "pr_binding_rejected:bound PR is not a Draft")
        self.assert_no_mutation(mocks)
        for head_repo in (None, {"nameWithOwner": f"{OWNER}/fork"}):
            with self.subTest(head_repo=head_repo):
                self.persisted.clear()
                self.seed_claim()
                with self.command_context(pr_candidate=candidate_pr(headRepository=head_repo)) as mocks:
                    result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
                self.assertEqual(result["reason"], "pr_binding_rejected:PR head repository is not the target repository")
                self.assert_no_mutation(mocks)

    def test_handoff_reads_issue_body_exactly_once(self):
        self.seed_claim()
        with self.command_context():
            self.assertTrue(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["handed_off"])
        self.assertEqual(self.body_reads, 1)


class TestHandoffWorkerStateAndCI(LocalHandoffBase):
    def test_success_records_worker_then_ci_then_monitor_receipt_then_dispatch(self):
        self.seed_claim()
        with self.command_context():
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertEqual(result, {
            "handed_off": True, "issue": ISSUE, "pr_number": PR,
            "head_sha": HEAD, "ci_run_id": RUN, "dispatch_id": DISPATCH_ID,
        })
        self.assertEqual(len(self.worker_records), 1)
        worker = self.worker_records[0]
        self.assertEqual(worker["worker_type"], "local-run")
        self.assertEqual(worker["pr_number"], PR)
        self.assertEqual(worker["head_sha"], HEAD)
        self.assertEqual(worker["extra"], {
            "branch": CANONICAL_BRANCH, "attempt_id": ATTEMPT,
            "dispatch_id": DISPATCH_ID, "claim_nonce": NONCE,
        })
        self.assertEqual(self.ci_acquisitions, [{"pr_number": PR, "head_sha": HEAD, "run_id": RUN, "source": "workflow_dispatch"}])
        self.assertEqual(self.ci_states, [{"pr_number": PR, "head_sha": HEAD, "run_id": RUN, "status": "dispatched"}])
        self.assertEqual(self.workflow_calls, [(
            dispatcher.MONITOR_WORKFLOW,
            {"issue": ISSUE, "pr": PR, "head_sha": HEAD, "ci_run_id": RUN},
        )])
        self.assertEqual(self.label_calls, [])
        order = [entry[0] for entry in self.mutation_order]
        self.assertEqual(order, [
            "record-worker", "record-ci-acquisition", "record-ci", "record-dispatch",
            "run-workflow", "record-dispatch",
        ])
        receipt = self.persisted[RECEIPT_ID]
        self.assertEqual(receipt["action"], "ci-monitor")
        self.assertEqual(receipt["status"], "dispatched")
        self.assertEqual(receipt["details"]["ci_run_id"], RUN)
        # The dispatched receipt is persisted only after the proven run request.
        self.assertEqual(self.records[0]["status"], "pending")
        self.assertEqual(self.records[1]["status"], "dispatched")
        self.assertLess(self.mutation_order.index(("run-workflow", "agent-ci-monitor.yml")),
                        self.mutation_order.index(("record-dispatch", "ci-monitor", "dispatched")))

    def test_worker_state_exact_retry_is_idempotent_and_conflict_fails_closed(self):
        self.seed_claim()
        with self.command_context(worker=worker_state()):
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertTrue(result["handed_off"])
        self.assertEqual(self.worker_records, [])
        self.assertEqual(self.mutation_order[0][0], "record-ci-acquisition")
        self.worker_records.clear()
        self.ci_acquisitions.clear()
        self.ci_states.clear()
        self.workflow_calls.clear()
        self.mutation_order.clear()
        self.persisted.clear()
        self.seed_claim()
        with self.command_context(worker=worker_state(head="f" * 40)) as mocks:
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertEqual(result["reason"], "worker_state_failed:conflicting_worker_state")
        self.assertEqual(self.ci_acquisitions, [])
        self.assertEqual(self.workflow_calls, [])
        mocks["gh"].assert_not_called()

    def test_worker_state_write_failure_fails_closed(self):
        self.seed_claim()
        with self.command_context(worker_write_ok=False) as mocks:
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertEqual(result["reason"], "worker_state_failed:worker_state_write_failed")
        self.assertEqual(self.ci_acquisitions, [])
        self.assertEqual(self.workflow_calls, [])
        mocks["gh"].assert_not_called()

    def test_ci_acquisition_is_reused_when_bound_with_exact_ci_state(self):
        self.seed_claim()
        existing = {
            "kind": "agent-orchestrator-ci-acquisition",
            "pr_number": PR, "head_sha": HEAD, "workflow_run_id": RUN,
            "source": "pull_request", "status": "bound",
        }
        with self.command_context(acquisition=existing, ci_read_outcome="matched",
                                  ci_state=ci_state_record()) as mocks:
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertTrue(result["handed_off"])
        self.assertEqual(result["ci_run_id"], RUN)
        self.assertEqual(self.ci_acquisitions, [])
        self.assertEqual(self.ci_states, [])
        self.assertEqual(len(self.workflow_calls), 1)
        mocks["gh"].assert_not_called()

    def test_ci_acquisition_reuse_persists_absent_exact_ci_state_before_monitor(self):
        self.seed_claim()
        existing = {
            "kind": "agent-orchestrator-ci-acquisition",
            "pr_number": PR, "head_sha": HEAD, "workflow_run_id": RUN,
            "source": "pull_request", "status": "bound",
        }
        with self.command_context(acquisition=existing) as mocks:
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertTrue(result["handed_off"])
        self.assertEqual(self.ci_acquisitions, [])
        self.assertEqual(self.ci_states, [{"pr_number": PR, "head_sha": HEAD, "run_id": RUN, "status": "dispatched"}])
        self.assertEqual(len(self.workflow_calls), 1)
        order = [entry[0] for entry in self.mutation_order]
        self.assertLess(order.index("record-ci"), order.index("run-workflow"))
        mocks["gh"].assert_not_called()

    def test_ci_acquisition_reuse_conflicting_or_unreadable_ci_state_fails_closed(self):
        self.seed_claim()
        existing = {
            "kind": "agent-orchestrator-ci-acquisition",
            "pr_number": PR, "head_sha": HEAD, "workflow_run_id": RUN,
            "source": "pull_request", "status": "bound",
        }
        for outcome in ("conflict", "unverifiable"):
            with self.subTest(outcome=outcome):
                self.ci_states.clear()
                self.workflow_calls.clear()
                with self.command_context(acquisition=existing, ci_read_outcome=outcome,
                                          ci_state=ci_state_record(run=RUN + 1)):
                    result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
                self.assertEqual(result["reason"], "ci_acquisition_failed")
                self.assertEqual(self.ci_states, [])
                self.assertEqual(self.workflow_calls, [])
        self.ci_states.clear()
        self.workflow_calls.clear()
        with self.command_context(acquisition=existing, ci_read_error=RuntimeError("api down")):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "ci_acquisition_failed")
        self.assertEqual(self.ci_states, [])
        self.assertEqual(self.workflow_calls, [])
        self.ci_states.clear()
        self.workflow_calls.clear()
        with self.command_context(acquisition=existing, ci_write_ok=False):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "ci_acquisition_failed")
        self.assertEqual(self.ci_states, [])
        self.assertEqual(self.workflow_calls, [])

    def test_ci_acquisition_conflict_or_failure_fails_closed(self):
        self.seed_claim()
        existing = {
            "kind": "agent-orchestrator-ci-acquisition",
            "pr_number": PR, "head_sha": HEAD, "workflow_run_id": RUN,
            "source": "pull_request", "status": "unbound",
        }
        with self.command_context(acquisition=existing):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "ci_acquisition_failed")
        with self.command_context(acquire_error=ci_verifier.CIVerificationError("absent")):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "ci_acquisition_failed")
        with self.command_context(acquire_result={"workflow_run_id": "not-an-int"}):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "ci_acquisition_failed")
        with self.command_context(acquire_result={
            "workflow_run_id": RUN, "source": "workflow_dispatch", "duplicate_run_ids": ["not-an-int"],
        }):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "ci_acquisition_failed")
        with self.command_context(acquisition_write_ok=False):
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "ci_acquisition_failed")
        self.assertEqual(self.workflow_calls, [])


class TestMonitorDispatchDedupe(LocalHandoffBase):
    def test_existing_exact_receipt_never_dispatches_twice(self):
        self.seed_claim()
        self.persisted[RECEIPT_ID] = monitor_receipt()
        with self.command_context():
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertTrue(result["handed_off"])
        self.assertEqual(self.workflow_calls, [])
        self.assertEqual(self.records, [])

    def test_conflicting_receipt_fails_closed(self):
        self.seed_claim()
        self.persisted[RECEIPT_ID] = monitor_receipt(run=RUN + 1)
        with self.command_context():
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertEqual(result["reason"], "monitor_receipt_conflict")
        self.assertEqual(self.workflow_calls, [])
        self.persisted[RECEIPT_ID] = state(ISSUE, RECEIPT_ID, "ci-monitor", "claimed", {})
        with self.command_context():
            self.assertEqual(dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)["reason"], "monitor_receipt_conflict")

    def test_ambiguous_dispatch_fails_closed_retains_outcome_unknown_and_never_redispatch(self):
        self.seed_claim()
        with self.command_context(run_workflow_ok=False) as mocks:
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertEqual(result["reason"], "monitor_dispatch_outcome_unknown")
        self.assertEqual(self.label_calls, [])
        self.assertEqual(self.persisted[RECEIPT_ID]["status"], "outcome_unknown")
        self.assertEqual(len(self.workflow_attempts), 1)
        mocks["gh"].assert_not_called()
        with self.command_context(run_workflow_ok=True) as mocks:
            retry = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertEqual(retry["reason"], "monitor_dispatch_outcome_unknown")
        self.assertFalse(retry["handed_off"])
        self.assertEqual(self.workflow_calls, [])
        self.assertEqual(len(self.workflow_attempts), 1)
        mocks["gh"].assert_not_called()

    def test_pending_or_outcome_unknown_receipt_fails_closed_never_success_or_redispatch(self):
        self.seed_claim()
        for status in ("pending", "outcome_unknown"):
            with self.subTest(status=status):
                self.persisted[RECEIPT_ID] = monitor_receipt(status=status)
                with self.command_context() as mocks:
                    result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
                self.assertEqual(result["reason"], "monitor_dispatch_outcome_unknown")
                self.assertEqual(self.workflow_calls, [])
                self.assertEqual(self.records, [])
                mocks["gh"].assert_not_called()
                self.persisted.pop(RECEIPT_ID, None)

    def test_receipt_write_failure_fails_closed_before_dispatch(self):
        self.seed_claim()
        with self.command_context(record_fail_dispatch=True) as mocks:
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertEqual(result["reason"], "monitor_receipt_failed")
        self.assertEqual(self.workflow_calls, [])
        mocks["gh"].assert_not_called()

    def test_post_success_receipt_write_failure_reports_outcome_unknown_no_second_dispatch(self):
        self.seed_claim()
        with self.command_context(record_fail_statuses={"dispatched"}) as mocks:
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertEqual(result["reason"], "monitor_dispatch_outcome_unknown")
        self.assertFalse(result["handed_off"])
        self.assertEqual(len(self.workflow_attempts), 1)
        self.assertEqual(self.persisted[RECEIPT_ID]["status"], "outcome_unknown")
        mocks["gh"].assert_not_called()
        self.workflow_calls.clear()
        self.workflow_attempts.clear()
        with self.command_context(record_fail_statuses={"dispatched"}):
            retry = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertEqual(retry["reason"], "monitor_dispatch_outcome_unknown")
        self.assertFalse(retry["handed_off"])
        self.assertEqual(self.workflow_calls, [])
        self.assertEqual(self.workflow_attempts, [])


class TestReleaseLocal(LocalHandoffBase):
    def test_release_persists_terminal_binding_before_label_change(self):
        self.seed_claim(claimed_state(status="dispatched"))
        with self.command_context() as mocks:
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertEqual(result, {"released": True, "issue": ISSUE, "dispatch_id": DISPATCH_ID, "reason": REASON})
        terminal = self.persisted[DISPATCH_ID]
        self.assertEqual(terminal["action"], "local-run")
        self.assertEqual(terminal["status"], "failed")
        self.assertEqual(terminal["details"]["reason"], REASON)
        for key in ("attempt_id", "client_token", "accepted_main_sha", "canonical_branch",
                    "lease_deadline", "allowed_paths", "task_body_sha256", "claim_nonce",
                    "previous_labels", "target_label"):
            self.assertEqual(terminal["details"][key], claim_details()[key])
        self.assertEqual(self.label_calls, [[sm.LABEL_BLOCKED]])
        order = [entry[0] for entry in self.mutation_order]
        self.assertLess(order.index("record-dispatch"), order.index("set-labels"))
        mocks["gh"].assert_not_called()

    def test_release_from_claimed_pre_handoff_state_succeeds(self):
        self.seed_claim(claimed_state(status="claimed"))
        with self.command_context():
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertTrue(result["released"])
        self.assertEqual(self.persisted[DISPATCH_ID]["status"], "failed")

    def test_release_exact_retry_is_idempotent(self):
        self.seed_claim(claimed_state(status="failed", **{"reason": REASON}))
        with self.command_context(labels=frozenset({sm.LABEL_BLOCKED})) as mocks:
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertTrue(result["released"])
        self.assertEqual(self.records, [])
        self.assertEqual(self.label_calls, [])
        mocks["gh"].assert_not_called()

    def test_release_local_refuses_unknown_output_terminal(self):
        self.seed_claim(
            claimed_state(status="failed_unknown_output", **{"reason": "local_unknown_output"})
        )
        with self.command_context(labels=frozenset({sm.LABEL_BLOCKED})):
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertEqual(result["reason"], "claim_state_unexpected")
        self.assertEqual(self.label_calls, [])

    def test_block_local_persists_unknown_output_before_agent_blocked(self):
        self.seed_claim(claimed_state(status="dispatched"))
        with self.command_context() as mocks:
            result = dispatcher.block_local(
                ISSUE, ATTEMPT, CLIENT_TOKEN, dispatcher.LOCAL_UNKNOWN_OUTPUT_REASON
            )
        self.assertEqual(result["blocked"], True)
        terminal = self.persisted[DISPATCH_ID]
        self.assertEqual(terminal["status"], "failed_unknown_output")
        self.assertEqual(terminal["details"]["reason"], dispatcher.LOCAL_UNKNOWN_OUTPUT_REASON)
        self.assertEqual(self.label_calls, [[sm.LABEL_BLOCKED]])
        order = [entry[0] for entry in self.mutation_order]
        self.assertLess(order.index("record-dispatch"), order.index("set-labels"))
        mocks["gh"].assert_not_called()

    def test_release_exact_terminal_retry_succeeds_after_lease_expiry(self):
        self.seed_claim(claimed_state(status="failed", lease_deadline="2020-01-01T00:00:00Z",
                                      **{"reason": REASON}))
        with self.command_context(labels=frozenset({sm.LABEL_BLOCKED})) as mocks:
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertTrue(result["released"])
        self.assertEqual(self.records, [])
        self.assertEqual(self.label_calls, [])
        mocks["gh"].assert_not_called()

    def test_release_exact_terminal_with_expired_lease_releases_active_capacity(self):
        self.seed_claim(claimed_state(status="failed", lease_deadline="2020-01-01T00:00:00Z",
                                      **{"reason": REASON}))
        with self.command_context(labels=frozenset({sm.LABEL_RUNNING})) as mocks:
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertTrue(result["released"])
        self.assertEqual(self.records, [])
        self.assertEqual(self.label_calls, [[sm.LABEL_BLOCKED]])
        mocks["gh"].assert_not_called()

    def test_release_active_claim_still_requires_live_lease(self):
        self.seed_claim(claimed_state(status="dispatched", lease_deadline="2020-01-01T00:00:00Z"))
        with self.command_context() as mocks:
            self.assertEqual(dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)["reason"],
                             "claim_lease_expired")
        self.assertEqual(self.records, [])
        self.assertEqual(self.label_calls, [])
        mocks["gh"].assert_not_called()

    def test_release_conflicting_terminal_fails_closed(self):
        self.seed_claim(claimed_state(status="failed", **{"reason": "local_aborted"}))
        with self.command_context() as mocks:
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertEqual(result["reason"], "conflicting_terminal_state")
        self.assertEqual(self.records, [])
        self.assertEqual(self.label_calls, [])
        mocks["gh"].assert_not_called()

    def test_release_terminal_write_failure_fails_closed_before_label(self):
        self.seed_claim(claimed_state(status="dispatched"))
        with self.command_context(record_fail_dispatch=True) as mocks:
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertEqual(result["reason"], "claim_state_failed_write")
        self.assertEqual(self.label_calls, [])
        mocks["gh"].assert_not_called()

    def test_release_label_failure_keeps_terminal_state(self):
        self.seed_claim(claimed_state(status="claimed"))
        with self.command_context(set_labels_ok=False):
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertEqual(result["reason"], "capacity_release_failed:label_transition_failed")
        self.assertEqual(self.persisted[DISPATCH_ID]["status"], "failed")

    def test_release_never_touches_ci_or_actions(self):
        self.seed_claim()
        with self.command_context() as mocks:
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertTrue(result["released"])
        self.assertEqual(self.ci_acquisitions, [])
        self.assertEqual(self.ci_states, [])
        self.assertEqual(self.workflow_calls, [])
        mocks["gh"].assert_not_called()

    def test_release_terminal_retry_must_not_release_newer_generation_capacity(self):
        # A stale retry terminalizes attempt A after a newer claim B owns the
        # Issue.  Pre-fix, the exact-id read validated A's failed terminal and
        # release_failed_capacity then demoted B's active label to
        # agent-blocked.  The generation guard must fail closed on
        # "superseded" before any terminal write or label mutation.
        a_terminal = claimed_state(status="failed", **{"reason": REASON})
        b = claimed_state(
            status="dispatched",
            dispatch_id=DISPATCH_ID_B,
            attempt_id=ATTEMPT_B,
            client_token=TOKEN_B,
            claim_nonce=NONCE_B,
        )
        self.seed_claim(a_terminal)
        with self.command_context(
            labels=frozenset({sm.LABEL_RUNNING}),
            comments=[trusted_comment(b), trusted_comment(a_terminal)],
        ) as mocks:
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertEqual(result, {"released": False, "issue": ISSUE, "reason": "superseded"})
        self.assertEqual(self.records, [])
        self.assertEqual(self.label_calls, [])
        mocks["gh"].assert_not_called()

    def test_release_active_claim_superseded_by_newer_generation_writes_nothing(self):
        # The same guard covers a first-time release of an active A when a
        # newer generation B already owns the Issue: no terminal is written
        # and no label is mutated.
        a_active = claimed_state(status="dispatched")
        b = claimed_state(
            status="dispatched",
            dispatch_id=DISPATCH_ID_B,
            attempt_id=ATTEMPT_B,
            client_token=TOKEN_B,
            claim_nonce=NONCE_B,
        )
        self.seed_claim(a_active)
        with self.command_context(
            labels=frozenset({sm.LABEL_RUNNING}),
            comments=[trusted_comment(b), trusted_comment(a_active)],
        ) as mocks:
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertEqual(result["released"], False)
        self.assertEqual(result["reason"], "superseded")
        self.assertEqual(self.records, [])
        self.assertEqual(self.label_calls, [])
        self.assertEqual(self.persisted[DISPATCH_ID]["status"], "dispatched")
        mocks["gh"].assert_not_called()

    def test_release_fails_closed_on_unreadable_or_malformed_newest_state(self):
        # API-unavailable newest state fails closed with no mutation.
        self.seed_claim(claimed_state(status="failed", **{"reason": REASON}))
        with self.command_context(comments_error=sm.StateUnavailableError("api down")) as mocks:
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertEqual(result["released"], False)
        self.assertEqual(result["reason"], "claim_state_unavailable")
        self.assertEqual(self.records, [])
        self.assertEqual(self.label_calls, [])
        mocks["gh"].assert_not_called()
        # An unparseable newest dispatch-state comment fails closed.
        self.persisted.clear()
        terminal = claimed_state(status="failed", **{"reason": REASON})
        self.seed_claim(terminal)
        malformed = {
            "author": {"login": "github-actions[bot]"},
            "body": '{"kind": "agent-orchestrator-dispatch-state", "status": "dispatched"',
        }
        with self.command_context(comments=[malformed, trusted_comment(terminal)]) as mocks:
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertEqual(result["released"], False)
        self.assertEqual(result["reason"], "claim_state_unverifiable")
        self.assertEqual(self.records, [])
        self.assertEqual(self.label_calls, [])
        mocks["gh"].assert_not_called()
        # A newer local-run claim generation missing its claim nonce fails
        # closed instead of being treated as a releasable older generation.
        self.persisted.clear()
        self.seed_claim(terminal)
        no_nonce = claimed_state(
            status="dispatched",
            dispatch_id=DISPATCH_ID_B,
            attempt_id=ATTEMPT_B,
            client_token=TOKEN_B,
            claim_nonce=NONCE_B,
        )
        del no_nonce["details"]["claim_nonce"]
        with self.command_context(comments=[trusted_comment(no_nonce), trusted_comment(terminal)]) as mocks:
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertEqual(result["released"], False)
        self.assertEqual(result["reason"], "claim_nonce_unavailable")
        self.assertEqual(self.records, [])
        self.assertEqual(self.label_calls, [])
        mocks["gh"].assert_not_called()

    def test_release_exact_terminal_retry_idempotent_with_no_newer_generation(self):
        # With only generation A present, the guard classifies the newest
        # relevant state as the caller's own exact terminal, preserving the
        # existing idempotent release path (no writes when already released).
        self.seed_claim(claimed_state(status="failed", **{"reason": REASON}))
        with self.command_context(labels=frozenset({sm.LABEL_BLOCKED})) as mocks:
            result = dispatcher.release_local(ISSUE, ATTEMPT, CLIENT_TOKEN, REASON)
        self.assertTrue(result["released"])
        self.assertEqual(self.records, [])
        self.assertEqual(self.label_calls, [])
        mocks["gh"].assert_not_called()


class TestReadExactCIStateDirect(unittest.TestCase):
    """Direct provider-free tests of the real ``state_manager.read_exact_ci_state``.

    Only the comment-API transport (``get_issue_comments``) is mocked; the
    helper's newest-first selection, fail-closed metadata checks, and the
    ``(pr_number, head_sha, workflow_run_id)`` binding classification all run
    for real, so the reuse path in ``_acquire_local_ci`` is exercised against
    the actual reader.
    """

    def read(self, comments=()):
        with mock.patch.object(sm, "get_issue_comments", return_value=comments):
            return sm.read_exact_ci_state(ISSUE, PR, HEAD, RUN, REPO)

    def test_newest_ordering_shadows_older_exact_match(self):
        # Comments arrive newest first: the newest CI state wins, so an older
        # comment bound to the exact (pr, head, run) cannot rescue a newer
        # state bound to a different run.
        older_match = ci_state_record()
        newest_conflict = ci_state_record(run=RUN + 1)
        outcome, state = self.read([trusted_comment(newest_conflict), trusted_comment(older_match)])
        self.assertEqual(outcome, "conflict")
        self.assertEqual(state, newest_conflict)

    def test_malformed_newest_ci_state_fails_closed(self):
        malformed = {
            "author": {"login": "github-actions[bot]"},
            "body": '{"kind": "agent-orchestrator-ci-state", "version": 2',
        }
        outcome, payload = self.read([malformed, trusted_comment(ci_state_record())])
        self.assertEqual((outcome, payload), ("unverifiable", "ci_state_malformed"))

    def test_unsupported_version_newest_ci_state_fails_closed(self):
        unsupported = ci_state_record()
        unsupported["version"] = 3
        outcome, payload = self.read([trusted_comment(unsupported)])
        self.assertEqual((outcome, payload), ("unverifiable", "ci_state_version_unsupported"))

    def test_wrong_kind_newest_ci_state_fails_closed(self):
        # The body carries the CI-state marker but parses to a different
        # document kind; it must fail closed rather than be skipped as
        # unrelated or treated as CI evidence.
        wrong_kind = ci_state_record()
        wrong_kind["kind"] = "agent-orchestrator-review-state"
        wrong_kind["summary"] = "agent-orchestrator-ci-state prose mention"
        outcome, payload = self.read([trusted_comment(wrong_kind)])
        self.assertEqual((outcome, payload), ("unverifiable", "ci_state_malformed"))

    def test_exact_match(self):
        expected = ci_state_record()
        outcome, payload = self.read([trusted_comment(expected)])
        self.assertEqual((outcome, payload), ("matched", expected))

    def test_conflict_on_any_binding_difference(self):
        for mismatched in (
            ci_state_record(pr=PR + 1),
            ci_state_record(head="c" * 40),
            ci_state_record(run=RUN + 1),
        ):
            with self.subTest(mismatched=mismatched):
                outcome, payload = self.read([trusted_comment(mismatched)])
                self.assertEqual((outcome, payload), ("conflict", mismatched))

    def test_absence_when_no_ci_state_comment_exists(self):
        outcome, payload = self.read([trusted_comment(ci_state_record(), author="alice")])
        self.assertEqual((outcome, payload), ("absent", None))

    def test_untrusted_newest_author_cannot_become_ci_evidence(self):
        # A human-authored comment can never become CI state evidence, even
        # when it is the newest.
        human = {"author": {"login": "alice"}, "body": "prose about agent-orchestrator-ci-state"}
        expected = ci_state_record()
        outcome, payload = self.read([human, trusted_comment(expected)])
        self.assertEqual((outcome, payload), ("matched", expected))

    def test_api_unavailable_fails_closed(self):
        with mock.patch.object(
            sm, "get_issue_comments", side_effect=sm.StateUnavailableError("api down")
        ):
            outcome, payload = sm.read_exact_ci_state(ISSUE, PR, HEAD, RUN, REPO)
        self.assertEqual((outcome, payload), ("unverifiable", "ci_state_unavailable"))

    def view_of(self, pr, **overrides):
        view = dict(pr)
        view.setdefault("headRepository", {"nameWithOwner": REPO})
        view.update(overrides)
        return view

    def find(self, prs, view):
        with mock.patch.object(pr_binding, "_open_prs", return_value=prs), \
             mock.patch.object(pr_binding, "_view_pr", return_value=view):
            return pr_binding.find_issue_pr(ISSUE, CANONICAL_BRANCH, HEAD, REPO)

    def test_authoritative_view_supplies_number_base_head_draft_and_repo(self):
        stale_list = [candidate_pr(baseRefName="develop", headRefOid="0" * 40)]
        result = self.find(stale_list, self.view_of(candidate_pr()))
        self.assertEqual(result["number"], PR)
        self.assertEqual(result["baseRefName"], "main")
        self.assertEqual(result["headRefOid"], HEAD)
        self.assertIs(result["isDraft"], True)
        self.assertEqual(result["headRepository"]["nameWithOwner"], REPO)

    def test_zero_or_multiple_candidates_fail_closed(self):
        with mock.patch.object(pr_binding, "_open_prs", return_value=[]):
            with self.assertRaises(pr_binding.PRBindingError):
                pr_binding.find_issue_pr(ISSUE, CANONICAL_BRANCH, HEAD, REPO)
        with mock.patch.object(pr_binding, "_open_prs", return_value=[candidate_pr(), candidate_pr(pr=PR + 1)]):
            with self.assertRaises(pr_binding.PRBindingError):
                pr_binding.find_issue_pr(ISSUE, CANONICAL_BRANCH, HEAD, REPO)

    def test_invalid_candidate_fails_closed_before_view(self):
        for bad in (candidate_pr(number="123"), candidate_pr(state="CLOSED")):
            with self.subTest(bad=bad):
                with mock.patch.object(pr_binding, "_open_prs", return_value=[bad]):
                    with self.assertRaises(pr_binding.PRBindingError):
                        pr_binding.find_issue_pr(ISSUE, CANONICAL_BRANCH, HEAD, REPO)

    def test_inconsistent_or_untrusted_final_view_fails_closed(self):
        for view in (
            self.view_of(candidate_pr(pr=PR + 1)),
            self.view_of(candidate_pr(state="CLOSED")),
            self.view_of(candidate_pr(isDraft=False)),
            self.view_of(candidate_pr(headRepository=None)),
            self.view_of(candidate_pr(headRepository={"nameWithOwner": f"{OWNER}/fork"})),
        ):
            with self.subTest(view=view):
                with mock.patch.object(pr_binding, "_open_prs", return_value=[candidate_pr()]), \
                     mock.patch.object(pr_binding, "_view_pr", return_value=view):
                    with self.assertRaises(pr_binding.PRBindingError):
                        pr_binding.find_issue_pr(ISSUE, CANONICAL_BRANCH, HEAD, REPO)

    def test_final_view_requires_exact_head_branch_and_default_base(self):
        for view in (
            self.view_of(candidate_pr(headRefOid="0" * 40)),
            self.view_of(candidate_pr(headRefName="agent/issue-99")),
            self.view_of(candidate_pr(baseRefName="develop")),
        ):
            with self.subTest(view=view):
                with mock.patch.object(pr_binding, "_open_prs", return_value=[candidate_pr()]), \
                     mock.patch.object(pr_binding, "_view_pr", return_value=view):
                    with self.assertRaises(pr_binding.PRBindingError):
                        pr_binding.find_issue_pr(ISSUE, CANONICAL_BRANCH, HEAD, REPO)

    def test_final_view_requires_binding_marker_and_closing_link(self):
        no_marker = candidate_pr(body=f"Closes #{ISSUE}\n")
        no_link = candidate_pr(
            body=f'<!-- agent-orchestrator-binding: {{"issue_number": {ISSUE}, "branch": "{CANONICAL_BRANCH}"}} -->'
        )
        for view in (self.view_of(no_marker), self.view_of(no_link)):
            with self.subTest(view=view):
                with mock.patch.object(pr_binding, "_open_prs", return_value=[candidate_pr()]), \
                     mock.patch.object(pr_binding, "_view_pr", return_value=view):
                    with self.assertRaises(pr_binding.PRBindingError):
                        pr_binding.find_issue_pr(ISSUE, CANONICAL_BRANCH, HEAD, REPO)


class TestCliAndWorkflowContract(LocalHandoffBase):
    def test_handoff_cli_prints_bounded_token_free_json(self):
        self.seed_claim()
        out = StringIO()
        with self.command_context(), \
             mock.patch.object(sys, "argv", ["dispatcher.py", "handoff-local", str(ISSUE), ATTEMPT, CLIENT_TOKEN, HEAD, NONCE]), \
             redirect_stdout(out):
            dispatcher.main()
        payload = json.loads(out.getvalue())
        self.assertEqual(payload["handed_off"], True)
        self.assertEqual(payload["pr_number"], PR)
        for secret in (CLIENT_TOKEN, NONCE, "client_token", "claim_nonce", "lease_deadline"):
            self.assertNotIn(secret, out.getvalue())

    def test_release_cli_prints_bounded_json_and_fails_exit_nonzero(self):
        self.seed_claim()
        out = StringIO()
        with self.command_context(), \
             mock.patch.object(sys, "argv", ["dispatcher.py", "release-local", str(ISSUE), ATTEMPT, CLIENT_TOKEN, REASON, NONCE]), \
             redirect_stdout(out):
            dispatcher.main()
        self.assertEqual(json.loads(out.getvalue())["released"], True)
        self.assertNotIn(CLIENT_TOKEN, out.getvalue())
        out = StringIO()
        with mock.patch.object(dispatcher, "_repo", return_value=REPO), \
             mock.patch.object(sys, "argv", ["dispatcher.py", "release-local", str(ISSUE), ATTEMPT, CLIENT_TOKEN, "free text", NONCE]), \
             redirect_stdout(out):
            with self.assertRaises(SystemExit) as ctx:
                dispatcher.main()
        self.assertEqual(ctx.exception.code, 1)
        self.assertEqual(json.loads(out.getvalue())["reason"], "invalid_reason_code")

    def test_cli_rejects_invalid_arity(self):
        for args in (
            ["dispatcher.py", "handoff-local", str(ISSUE), ATTEMPT],
            ["dispatcher.py", "handoff-local", str(ISSUE), ATTEMPT, CLIENT_TOKEN],
            ["dispatcher.py", "handoff-local", str(ISSUE), ATTEMPT, CLIENT_TOKEN, HEAD, "extra"],
            ["dispatcher.py", "release-local", str(ISSUE), ATTEMPT, CLIENT_TOKEN],
            ["dispatcher.py", "release-local", str(ISSUE), ATTEMPT, CLIENT_TOKEN, REASON],
            ["dispatcher.py", "release-local", str(ISSUE), ATTEMPT, CLIENT_TOKEN, REASON, "extra"],
            ["dispatcher.py", "block-local", str(ISSUE), ATTEMPT, CLIENT_TOKEN, dispatcher.LOCAL_UNKNOWN_OUTPUT_REASON],
            ["dispatcher.py", "block-local", str(ISSUE), ATTEMPT, CLIENT_TOKEN, dispatcher.LOCAL_UNKNOWN_OUTPUT_REASON, "extra"],
        ):
            with self.subTest(args=args):
                with mock.patch.object(sys, "argv", args):
                    with self.assertRaises(SystemExit):
                        dispatcher.main()

    def test_workflow_declares_bounded_commands_and_inputs_in_global_lane(self):
        source = (WORKFLOWS / "agent-controller.yml").read_text()
        for command in ("handoff-local", "release-local", "block-local"):
            self.assertIn(f"          - {command}\n", source)
        self.assertIn("      reason_code:\n", source)
        self.assertIn("INPUT_REASON_CODE: ${{ inputs.reason_code }}", source)
        self.assertIn("group: agent-dispatch-global", source)
        self.assertIn("cancel-in-progress: false", source)
        handoff_branch = source.split("handoff-local)", 1)[1].split(";;", 1)[0]
        self.assertIn("control_state.py require-live", handoff_branch)
        self.assertIn("dispatcher.py handoff-local", handoff_branch)
        self.assertEqual(handoff_branch.count("${{ inputs."), 1)
        self.assertIn('"$INPUT_ATTEMPT_ID" "$INPUT_CLIENT_TOKEN" "$INPUT_HEAD_SHA" "$INPUT_CLAIM_NONCE"', handoff_branch)
        self.assertNotIn("${{ inputs.attempt_id }}", handoff_branch)
        self.assertNotIn("${{ inputs.client_token }}", handoff_branch)
        self.assertNotIn("${{ inputs.head_sha }}", handoff_branch)
        release_branch = source.split("release-local)", 1)[1].split(";;", 1)[0]
        self.assertIn("dispatcher.py release-local", release_branch)
        self.assertEqual(release_branch.count("${{ inputs."), 1)
        self.assertIn('"$INPUT_ATTEMPT_ID" "$INPUT_CLIENT_TOKEN" "$INPUT_REASON_CODE" "$INPUT_CLAIM_NONCE"', release_branch)
        self.assertNotIn("${{ inputs.reason_code }}", release_branch)
        # Bounded strings reach the script only through env mappings.
        self.assertEqual(source.count("${{ inputs.attempt_id }}"), 1)
        self.assertEqual(source.count("${{ inputs.client_token }}"), 1)
        self.assertEqual(source.count("${{ inputs.reason_code }}"), 1)
        self.assertEqual(source.count("${{ inputs.head_sha }}"), 1)

    def test_monitor_dispatch_mapping_matches_actual_workflow_input_keys(self):
        self.seed_claim()
        with self.command_context():
            result = dispatcher.handoff_local(ISSUE, ATTEMPT, CLIENT_TOKEN, HEAD)
        self.assertTrue(result["handed_off"])
        fields = dict(self.workflow_calls[0][1])
        declared = set(monitor_workflow_input_keys())
        self.assertEqual(declared, {"issue", "pr", "head_sha", "ci_run_id"})
        self.assertEqual(set(fields), declared)
        receipt = self.persisted[RECEIPT_ID]
        paired = {
            "issue": receipt["details"]["issue_number"],
            "pr": receipt["details"]["pr_number"],
            "head_sha": receipt["details"]["head_sha"],
            "ci_run_id": receipt["details"]["ci_run_id"],
        }
        self.assertEqual(paired, fields)
        self.assertEqual(receipt["details"]["workflow"], dispatcher.MONITOR_WORKFLOW)


if __name__ == "__main__":
    unittest.main()
