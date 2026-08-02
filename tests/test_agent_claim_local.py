"""Provider-free tests for the trusted GitHub-serialized local claim gateway.

``dispatcher.claim_local`` is the server-side claim gate for a local process:
it derives repository/default-branch/accepted-main authority from GitHub,
never from the local caller, reads the Issue body exactly once, claims through
the existing ``_claim`` owners with action ``local-run``, requires a trusted
readback of the claimed comment before any label mutation, and records
``dispatched`` without ever starting a GitHub workflow.
"""

from __future__ import annotations

import contextlib
import hashlib
import json
import pathlib
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

import control_state  # noqa: E402
import dispatcher  # noqa: E402
import local_loop  # noqa: E402
import state_manager as sm  # noqa: E402


MAIN_SHA = "a" * 40
OWNER = "acme"
REPO = f"{OWNER}/repo"
ATTEMPT = "123e4567-e89b-12d3-a456-426614174000"
TOKEN = "c" * 32
SCOPE = '<!-- agent-orchestrator-scope:v1 {"allowed_paths":["src/"]} -->'
TASK = f'<!-- repo-agent-task:v1 {{"accepted_main_sha":"{MAIN_SHA}"}} -->'
BODY = f"{SCOPE}\n{TASK}"
ISSUE = 77
CANONICAL_BRANCH = f"agent/issue-{ISSUE}"
DIGEST = hashlib.sha256(BODY.encode("utf-8")).hexdigest()
DISPATCH_ID = f"local-run:{ISSUE}:{ATTEMPT}"


def state(issue, dispatch_id, action, status, details):
    return {
        "kind": "agent-orchestrator-dispatch-state",
        "version": 1,
        "issue_number": issue,
        "dispatch_id": dispatch_id,
        "action": action,
        "status": status,
        "details": dict(details),
    }


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


class ClaimLocalBase(unittest.TestCase):
    def setUp(self):
        self.records = []
        self.persisted = {}
        self.label_calls = []
        self.body_reads = 0

    @contextlib.contextmanager
    def claim_context(self, **overrides):
        """Enter the full mock set; kwargs override individual inputs.

        ``active`` is a tuple of snapshots consumed in call order: the
        pre-claim capacity read and the post-label recheck.  ``dependencies``
        defaults to ``"real"`` so the real precomputed-body dependency path is
        exercised; pass ``(ok, blocker)`` to force a dependency outcome.
        """
        repo_value = overrides.get("repo_value", REPO)
        live = overrides.get("live", True)
        metadata = overrides.get("metadata")
        main_sha = overrides.get("main_sha", MAIN_SHA)
        metadata_error = overrides.get("metadata_error")
        sha_error = overrides.get("sha_error")
        adapter_error = overrides.get("adapter_error")
        author = overrides.get("author", OWNER)
        body = overrides.get("body", BODY)
        labels = overrides.get("labels", frozenset({sm.LABEL_READY}))
        dependencies = overrides.get("dependencies", "real")
        has_pr = overrides.get("has_pr", False)
        active = overrides.get("active", (set(), set()))
        active_scopes = overrides.get("active_scopes", {})
        record_ok = overrides.get("record_ok", True)
        set_labels_ok = overrides.get("set_labels_ok", True)
        read_patch = overrides.get("read_patch", True)
        comments = overrides.get("comments")

        def read_state(_issue, dispatch_id, _repo=""):
            return self.persisted.get(dispatch_id)

        def record_state(_issue, dispatch_id, action, status, details=None, _repo=""):
            written = state(_issue, dispatch_id, action, status, details)
            self.records.append(written)
            self.persisted[dispatch_id] = written
            return True

        def counting_body(_issue, _repo=""):
            self.body_reads += 1
            return body

        def set_labels_mock(_issue, *new_labels, repo=""):
            self.label_calls.append(list(new_labels))
            return True

        with contextlib.ExitStack() as stack:
            stack.enter_context(mock.patch.object(dispatcher, "_repo", return_value=repo_value))
            stack.enter_context(mock.patch.object(
                dispatcher.control_state, "require_live",
                **({"return_value": {}} if live else {"side_effect": control_state.ControlStateError("stopped")}),
            ))
            if adapter_error is not None:
                def adapter_cls(_repo):
                    raise adapter_error
            else:
                fake = FakeAdapter(metadata=metadata, main_sha=main_sha,
                                   metadata_error=metadata_error, sha_error=sha_error)
                adapter_cls = lambda _repo: fake  # noqa: E731
            stack.enter_context(mock.patch.object(dispatcher.local_loop, "GitHubAdapter", side_effect=adapter_cls))
            gh = stack.enter_context(mock.patch.object(dispatcher.sm, "_gh"))
            stack.enter_context(mock.patch.object(dispatcher.sm, "get_issue_author", return_value=author))
            stack.enter_context(mock.patch.object(dispatcher.sm, "get_issue_body", side_effect=counting_body))
            if read_patch:
                stack.enter_context(mock.patch.object(dispatcher.sm, "read_dispatch_state", side_effect=read_state))
            stack.enter_context(mock.patch.object(dispatcher.sm, "get_issue_labels_checked", return_value=labels))
            deps = None
            if dependencies != "real":
                deps = stack.enter_context(mock.patch.object(
                    dispatcher.sm, "check_dependencies_complete", return_value=dependencies,
                ))
            stack.enter_context(mock.patch.object(dispatcher.sm, "has_open_issue_pr", return_value=has_pr))
            active_mock = stack.enter_context(mock.patch.object(
                dispatcher.sm, "get_active_issue_numbers", side_effect=list(active),
            ))
            stack.enter_context(mock.patch.object(dispatcher.sm, "get_active_issue_scopes", return_value=active_scopes))
            record_mock = stack.enter_context(mock.patch.object(dispatcher.sm, "record_dispatch_state"))
            record_mock.side_effect = record_state if record_ok else lambda *a, **k: False
            if comments is not None:
                stack.enter_context(mock.patch.object(dispatcher.sm, "get_issue_comments", return_value=comments))
            labels_obj = stack.enter_context(mock.patch.object(dispatcher.sm, "set_labels"))
            labels_obj.side_effect = set_labels_mock if set_labels_ok else lambda *a, **k: False
            workflow_mock = stack.enter_context(mock.patch.object(dispatcher, "_run_workflow"))
            yield {
                "gh": gh, "deps": deps, "active": active_mock,
                "record": record_mock, "set_labels": labels_obj, "workflow": workflow_mock,
            }

    def claimed_state(self, status="claimed", **details_overrides):
        details = {
            "issue_number": ISSUE,
            "attempt_id": ATTEMPT,
            "client_token": TOKEN,
            "accepted_main_sha": MAIN_SHA,
            "canonical_branch": CANONICAL_BRANCH,
            "lease_deadline": (datetime.now(timezone.utc) + timedelta(hours=4)).isoformat().replace("+00:00", "Z"),
            "previous_labels": [sm.LABEL_READY],
            "target_label": sm.LABEL_RUNNING,
            "allowed_paths": ["src/"],
            "task_body_sha256": DIGEST,
            "claim_nonce": "d" * 32,
            **details_overrides,
        }
        return state(ISSUE, DISPATCH_ID, "local-run", status, details)


class TestClaimLocalInputValidation(ClaimLocalBase):
    def test_invalid_attempt_id_fails_closed(self):
        for bad in ("not-a-uuid", "z" * 32, "0123456789abcdef", "", "urn:uuid:" + ATTEMPT,
                    "{" + ATTEMPT + "}", ATTEMPT.upper(), ATTEMPT.replace("-", "")):
            with self.subTest(attempt=bad):
                self.assertEqual(dispatcher.claim_local(ISSUE, bad, TOKEN),
                                 {"dispatched": False, "issue": ISSUE, "reason": "invalid_attempt_id"})

    def test_invalid_client_token_fails_closed(self):
        for bad in ("z" * 32, "C" * 32, "c" * 31, "c" * 33, ""):
            with self.subTest(token=bad):
                self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, bad),
                                 {"dispatched": False, "issue": ISSUE, "reason": "invalid_client_token"})


class TestClaimLocalServerDerivation(ClaimLocalBase):
    def test_control_and_repository_negatives_fail_closed(self):
        with self.claim_context(live=False):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "disabled_or_emergency_stopped")
        with self.claim_context(repo_value=""):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "repository_unavailable")
        with self.claim_context(adapter_error=ValueError("repository must be owner/name")):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "repository_malformed")
        with self.claim_context(metadata_error=local_loop.LoopUnavailable("boom")):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "repository_state_unavailable")
        with self.claim_context(metadata={"name_with_owner": REPO, "default_branch": "main"}):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "repository_state_unavailable")
        for branch in ("bad branch!", "x" * 201, ""):
            with self.subTest(branch=branch):
                with self.claim_context(metadata={"name_with_owner": REPO, "owner": OWNER, "default_branch": branch}):
                    self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "default_branch_unavailable")
        with self.claim_context(sha_error=local_loop.LoopUnavailable("boom")):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "accepted_main_unavailable")
        with self.claim_context(main_sha="not-a-sha"):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "accepted_main_unavailable")

    def test_author_and_task_binding_negatives_fail_closed(self):
        with self.claim_context(author=None):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "issue_state_unavailable")
        with self.claim_context(author="mallory"):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "untrusted_author")
        with self.claim_context(body=None):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "task_body_unavailable")
        for bad_body in ("no marker", TASK + TASK, '<!-- repo-agent-task:v1 {not json} -->',
                         f'<!-- repo-agent-task:v1 {{"accepted_main_sha":"{"B" * 40}"}} -->',
                         '<!-- repo-agent-task:v1 {"accepted_main_sha":"' + MAIN_SHA + '","extra":1} -->'):
            with self.subTest(body=bad_body):
                with self.claim_context(body=bad_body):
                    self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "invalid_task_binding")
        with self.claim_context(body=f"{SCOPE}\n<!-- repo-agent-task:v1 {{\"accepted_main_sha\":\"{'b' * 40}\"}} -->"):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "accepted_main_mismatch")

    def test_invalid_scope_writes_no_state_label_or_workflow(self):
        with self.claim_context(body=TASK) as mocks:
            result = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        self.assertTrue(result["reason"].startswith("invalid_scope:"))
        self.assertEqual(mocks["record"].call_count, 0)
        self.assertEqual(self.label_calls, [])
        mocks["workflow"].assert_not_called()
        mocks["gh"].assert_not_called()

    def test_dependencies_pr_labels_capacity_and_conflict_fail_closed(self):
        with self.claim_context(dependencies=(False, 42)):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "dependencies_not_ready:42")
        with self.claim_context(dependencies=(False, "dependency_state_unavailable")):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"],
                             "dependencies_not_ready:dependency_state_unavailable")
        with self.claim_context(has_pr=True):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "issue_already_associated")
        with self.claim_context(has_pr=None):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "association_state_unavailable")
        with self.claim_context(labels=None):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "label_state_unavailable")
        for labels in (frozenset(), frozenset({sm.LABEL_RUNNING}), frozenset({sm.LABEL_COMPLETE})):
            with self.subTest(labels=labels):
                with self.claim_context(labels=labels):
                    self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "issue_not_ready")
        with self.claim_context(active=({55, 66},)):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "capacity_full")
        with self.claim_context(active=(None,)):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "capacity_state_unavailable")
        with self.claim_context(active=({55},), active_scopes={55: ["src/"]}):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "scope_conflict:55")
        with self.claim_context(active=({55},), active_scopes=None):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "active_scope_state_unavailable")


class TestClaimLocalTrustedReadback(ClaimLocalBase):
    def test_trusted_readback_happens_before_label_and_before_dispatched(self):
        with self.claim_context():
            result = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        self.assertTrue(result["dispatched"])
        self.assertEqual([r["status"] for r in self.records], ["claimed", "dispatched"])
        self.assertEqual(self.label_calls, [[sm.LABEL_RUNNING]])

    def test_human_authored_direct_invocation_cannot_change_labels(self):
        forged = self.claimed_state(status="dispatched")
        with self.claim_context(read_patch=False,
                                comments=[{"author": {"login": "mallory"}, "body": json.dumps(forged, sort_keys=True)}]) as mocks:
            result = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        self.assertEqual(result["reason"], "claim_readback_unverified")
        self.assertEqual(self.label_calls, [])
        mocks["workflow"].assert_not_called()

    def test_human_forgery_cannot_mask_an_existing_trusted_claim(self):
        trusted = self.claimed_state()
        comments = [{"author": {"login": "mallory"}, "body": json.dumps(self.claimed_state(status="dispatched"), sort_keys=True)},
                    {"author": {"login": "github-actions[bot]"}, "body": json.dumps(trusted, sort_keys=True)}]
        with self.claim_context(read_patch=False, comments=comments):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "dispatch_in_flight")

    def test_malformed_trusted_comment_fails_closed(self):
        comments = [{"author": {"login": "github-actions[bot]"}, "body": "agent-orchestrator-dispatch-state {not json"}]
        with self.claim_context(read_patch=False, comments=comments):
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "dispatch_state_unavailable")


class TestClaimLocalSuccessContract(ClaimLocalBase):
    def test_success_persists_complete_claim_binding_and_dispatched_without_workflow(self):
        with self.claim_context() as mocks:
            result = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        self.assertEqual(result, {"dispatched": True, "issue": ISSUE, "dispatch_id": DISPATCH_ID})
        claimed = self.records[0]
        self.assertEqual(claimed["action"], "local-run")
        self.assertEqual(claimed["status"], "claimed")
        details = claimed["details"]
        self.assertEqual(set(details), {
            "issue_number", "attempt_id", "client_token", "accepted_main_sha",
            "canonical_branch", "lease_deadline", "previous_labels", "target_label",
            "allowed_paths", "task_body_sha256", "claim_nonce",
        })
        self.assertEqual(details["issue_number"], ISSUE)
        self.assertEqual(details["attempt_id"], ATTEMPT)
        self.assertEqual(details["client_token"], TOKEN)
        self.assertEqual(details["accepted_main_sha"], MAIN_SHA)
        self.assertEqual(details["canonical_branch"], CANONICAL_BRANCH)
        self.assertEqual(details["allowed_paths"], ["src/"])
        self.assertEqual(details["task_body_sha256"], DIGEST)
        self.assertEqual(details["previous_labels"], [sm.LABEL_READY])
        self.assertEqual(details["target_label"], sm.LABEL_RUNNING)
        self.assertRegex(details["claim_nonce"], r"^[0-9a-f]{32}$")
        dispatched = self.records[1]
        self.assertEqual(dispatched["status"], "dispatched")
        self.assertEqual(dispatched["details"]["workflow"], "local-run")
        for key in ("attempt_id", "client_token", "accepted_main_sha", "canonical_branch",
                    "lease_deadline", "allowed_paths", "task_body_sha256", "claim_nonce"):
            self.assertEqual(dispatched["details"][key], details[key])
        mocks["workflow"].assert_not_called()
        mocks["gh"].assert_not_called()

    def test_absent_remote_branch_is_never_required_and_body_is_read_once(self):
        with self.claim_context():
            self.assertTrue(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["dispatched"])
        self.assertEqual(self.body_reads, 1)
        self.assertEqual(self.records[0]["details"]["task_body_sha256"], DIGEST)

    def test_lease_deadline_is_bounded_utc_zulu_in_the_claim(self):
        before = datetime.now(timezone.utc)
        with self.claim_context():
            dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        raw = self.records[0]["details"]["lease_deadline"]
        self.assertTrue(raw.endswith("Z"))
        deadline = datetime.fromisoformat(raw)
        self.assertEqual(deadline.utcoffset(), timedelta(0))
        self.assertGreaterEqual(deadline, before)
        self.assertLessEqual(deadline - before, timedelta(hours=4) + timedelta(minutes=1))
        self.assertEqual(sm.LOCAL_CLAIM_LEASE_HOURS, 4)

    def test_claim_state_write_failure_fails_closed_before_label(self):
        with self.claim_context(record_ok=False) as mocks:
            result = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        self.assertEqual(result["reason"], "claim_state_failed")
        self.assertEqual(self.label_calls, [])
        mocks["workflow"].assert_not_called()

    def test_label_mutation_failure_records_failed_claim_with_binding(self):
        with self.claim_context(set_labels_ok=False) as mocks:
            result = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        self.assertEqual(result["reason"], "claim_label_failed")
        mocks["workflow"].assert_not_called()
        failed = self.records[-1]
        self.assertEqual(failed["status"], "failed")
        self.assertEqual(failed["details"]["reason"], "claim_label_failed")
        self.assertIn("client_token", failed["details"])

    def test_dispatched_record_failure_keeps_existing_claimed_binding(self):
        calls = []

        def record_state(_issue, dispatch_id, action, status, details=None, _repo=""):
            calls.append(status)
            if len(calls) == 2:
                return False
            written = state(_issue, dispatch_id, action, status, details)
            self.records.append(written)
            self.persisted[dispatch_id] = written
            return True

        with self.claim_context() as mocks:
            mocks["record"].side_effect = record_state
            result = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        self.assertEqual(result["reason"], "dispatch_state_failed_capacity_retained")
        self.assertEqual(self.label_calls, [[sm.LABEL_RUNNING]])
        self.assertEqual(self.records[0]["status"], "claimed")
        with self.claim_context():
            self.assertEqual(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["reason"], "dispatch_in_flight")

    def test_capacity_recheck_exceeded_or_unavailable_compensates(self):
        with self.claim_context(active=(set(), {ISSUE, 55, 66})):
            result = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        self.assertEqual(result["reason"], "capacity_recheck_exceeded")
        rollback = next(r for r in self.records if r["action"] == "rollback")
        self.assertEqual(rollback["status"], "failed")
        self.assertEqual(rollback["details"]["reason"], "capacity_recheck_exceeded")
        self.assertRegex(rollback["details"]["claim_nonce"], r"^[0-9a-f]{32}$")
        self.assertEqual(rollback["details"]["client_token"], TOKEN)
        self.assertEqual(self.label_calls[-1], [sm.LABEL_READY])
        self.records.clear()
        with self.claim_context(active=(set(), None)):
            result = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        self.assertEqual(result["reason"], "capacity_recheck_unavailable")
        self.assertEqual(next(r for r in self.records if r["action"] == "rollback")["details"]["reason"],
                         "capacity_recheck_unavailable")


class TestClaimLocalRetrySemantics(ClaimLocalBase):
    def test_exact_retry_after_dispatched_is_idempotent_and_in_flight_otherwise(self):
        self.persisted[DISPATCH_ID] = self.claimed_state(status="dispatched")
        with self.claim_context() as mocks:
            result = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        self.assertEqual(result, {"dispatched": True, "issue": ISSUE, "reason": "already_dispatched"})
        self.assertEqual(mocks["record"].call_count, 0)
        self.assertEqual(self.label_calls, [])
        self.persisted.clear()
        self.persisted[DISPATCH_ID] = self.claimed_state()
        with self.claim_context() as mocks:
            result = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        self.assertEqual(result["dispatched"], False)
        self.assertEqual(result["reason"], "dispatch_in_flight")
        self.assertEqual(mocks["record"].call_count, 0)
        self.assertEqual(self.label_calls, [])

    def test_binding_field_order_is_irrelevant_to_retry_verification(self):
        prior = self.claimed_state()
        prior["details"] = dict(reversed(list(prior["details"].items())))
        self.persisted[prior["dispatch_id"]] = prior
        with self.claim_context() as mocks:
            result = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        self.assertEqual(result["reason"], "dispatch_in_flight")
        self.assertEqual(mocks["record"].call_count, 0)
        self.assertEqual(self.label_calls, [])

    def test_same_attempt_different_client_token_or_main_fails_closed(self):
        for status in ("claimed", "dispatched"):
            with self.subTest(status=status, changed="token"):
                self.persisted.clear()
                self.persisted[DISPATCH_ID] = self.claimed_state(status=status, client_token="e" * 32)
                with self.claim_context() as mocks:
                    result = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
                self.assertEqual(result["reason"], "dispatch_state_binding_unverified")
                self.assertEqual(mocks["record"].call_count, 0)
                self.assertEqual(self.label_calls, [])
        self.persisted.clear()
        self.persisted[DISPATCH_ID] = self.claimed_state(accepted_main_sha="f" * 40)
        with self.claim_context() as mocks:
            result = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        self.assertEqual(result["reason"], "dispatch_state_binding_unverified")
        self.assertEqual(mocks["record"].call_count, 0)

    def test_retry_preserves_original_lease_scope_digest_and_nonce(self):
        with self.claim_context():
            self.assertTrue(dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)["dispatched"])
        original = dict(self.records[0]["details"])
        label_snapshot = list(self.label_calls)
        with self.claim_context():
            second = dispatcher.claim_local(ISSUE, ATTEMPT, TOKEN)
        self.assertEqual(second["reason"], "already_dispatched")
        self.assertEqual(self.records[0]["details"], original)
        self.assertEqual(self.label_calls, label_snapshot)

    def test_claim_defaults_preserve_dispatch_ready_behavior(self):
        with mock.patch.object(
            dispatcher.sm, "read_task_scope_binding",
            return_value=(True, {"allowed_paths": ["src/"], "task_body_sha256": DIGEST}),
        ) as scope_read:
            with self.claim_context():
                claimed, _, reason = dispatcher._claim(
                    ISSUE, sm.LABEL_RUNNING, f"worker:{ISSUE}", "worker",
                    {"issue_number": ISSUE, "claim_nonce": "f" * 32},
                )
        self.assertTrue(claimed)
        self.assertEqual(reason, "claimed")
        scope_read.assert_called_once_with(ISSUE, REPO)


class TestClaimLocalCliAndWiring(ClaimLocalBase):
    def test_cli_prints_bounded_token_free_json_on_success(self):
        out = StringIO()
        with self.claim_context(), \
             mock.patch.object(sys, "argv", ["dispatcher.py", "claim-local", str(ISSUE), ATTEMPT, TOKEN]), \
             redirect_stdout(out):
            dispatcher.main()
        self.assertEqual(json.loads(out.getvalue()),
                         {"dispatched": True, "issue": ISSUE, "dispatch_id": DISPATCH_ID})
        for secret in (TOKEN, "client_token", "claim_nonce", "lease_deadline"):
            self.assertNotIn(secret, out.getvalue())

    def test_cli_failure_exits_nonzero_with_bounded_json(self):
        out = StringIO()
        with mock.patch.object(dispatcher, "_repo", return_value=REPO), \
             mock.patch.object(sys, "argv", ["dispatcher.py", "claim-local", str(ISSUE), "not-a-uuid", TOKEN]), \
             redirect_stdout(out):
            with self.assertRaises(SystemExit) as ctx:
                dispatcher.main()
        self.assertEqual(ctx.exception.code, 1)
        self.assertEqual(json.loads(out.getvalue())["reason"], "invalid_attempt_id")
        self.assertNotIn(TOKEN, out.getvalue())

    def test_cli_rejects_invalid_arity(self):
        with mock.patch.object(sys, "argv", ["dispatcher.py", "claim-local", str(ISSUE), ATTEMPT]):
            with self.assertRaises(SystemExit):
                dispatcher.main()

    def test_workflow_keeps_existing_global_lane_with_only_bounded_inputs(self):
        source = (WORKFLOWS / "agent-controller.yml").read_text()
        self.assertIn("          - claim-local\n", source)
        self.assertIn("      attempt_id:\n", source)
        self.assertIn("      client_token:\n", source)
        self.assertEqual(source.count("${{ inputs.attempt_id }}"), 1)
        self.assertEqual(source.count("${{ inputs.client_token }}"), 1)
        branch = source.split("claim-local)", 1)[1].split(";;", 1)[0]
        self.assertIn("control_state.py require-live", branch)
        self.assertIn("dispatcher.py claim-local", branch)
        self.assertEqual(branch.count("${{ inputs."), 1)
        self.assertEqual(branch.count("${{ inputs.issue }}"), 1)
        self.assertIn("$INPUT_ATTEMPT_ID", branch)
        self.assertIn("$INPUT_CLIENT_TOKEN", branch)
        self.assertNotIn("agent-worker", branch)
        self.assertNotIn("claim_nonce", branch)
        self.assertNotIn("lease", branch)
        self.assertIn("INPUT_ATTEMPT_ID: ${{ inputs.attempt_id }}", source)
        self.assertIn("INPUT_CLIENT_TOKEN: ${{ inputs.client_token }}", source)
        self.assertIn("group: agent-dispatch-global", source)
        self.assertIn("cancel-in-progress: false", source)
        self.assertNotIn("cancel-in-progress: ${{ inputs.command == 'emergency-stop' }}", source)
        self.assertNotIn('"${{ inputs.client_token }}"', source)
        self.assertNotIn('"${{ inputs.attempt_id }}"', source)

    def test_loopctl_has_no_local_claim_or_mutation_surface(self):
        loopctl_source = (CONTROL / "loopctl.py").read_text()
        self.assertIn('subparsers.add_parser("poll"', loopctl_source)
        self.assertNotIn("claim", loopctl_source)


if __name__ == "__main__":
    unittest.main()
