"""Executable regressions for bounded orchestrator review finalization."""

from __future__ import annotations

import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
CONTROL = ROOT / "scripts" / "agent-control"
sys.path.insert(0, str(CONTROL))

import state_manager as sm  # type: ignore[import-not-found]


HEAD = "a" * 40
OLD_HEAD = "b" * 40


def review_payload(verdict="PASS", **overrides):
    payload = {
        "verdict": verdict,
        "summary": f"{verdict} review result",
        "reviewed_head_sha": HEAD,
        "ci_green": True,
        "security_ok": True,
        "rollback_ok": True,
        "blockers": [],
        "major_notes": ["bounded major note"],
        "minor_notes": ["bounded minor note"],
    }
    payload.update(overrides)
    return payload


def parse_outputs(path):
    output = {}
    lines = path.read_text().splitlines()
    index = 0
    while index < len(lines):
        key, delimiter = lines[index].split("<<", 1)
        index += 1
        value = []
        while lines[index] != delimiter:
            value.append(lines[index])
            index += 1
        output[key] = "\n".join(value)
        index += 1
    return output


class ValidatorHarness:
    def __init__(self, payload=None, raw=None, exists=True):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.artifact = self.root / "review.json"
        self.sidecar = self.root / "validation.json"
        self.output = self.root / "github-output"
        if exists:
            if raw is None:
                raw = json.dumps(payload).encode()
            self.artifact.write_bytes(raw)

    def run(self):
        env = {**os.environ, "GITHUB_OUTPUT": str(self.output), "GITHUB_RUN_ID": "91234"}
        result = subprocess.run(
            [
                sys.executable,
                str(CONTROL / "validate_review.py"),
                str(self.artifact),
                "207",
                HEAD,
                str(self.sidecar),
            ],
            cwd=ROOT,
            env=env,
            text=True,
            capture_output=True,
            timeout=30,
        )
        return result, json.loads(self.sidecar.read_text()), parse_outputs(self.output)

    def close(self):
        self.directory.cleanup()


class TestReviewValidatorBehavior(unittest.TestCase):
    def test_every_schema_valid_business_verdict_succeeds_and_preserves_evidence(self):
        for verdict in ("PASS", "PASS_WITH_NOTES", "BLOCKED", "FAIL"):
            with self.subTest(verdict=verdict):
                # Review Convergence: PASS/PASS_WITH_NOTES must not carry blockers;
                # BLOCKED requires at least one blocker. FAIL may record defects.
                if verdict in {"PASS", "PASS_WITH_NOTES"}:
                    blockers = []
                else:
                    blockers = ["blocked"]
                harness = ValidatorHarness(review_payload(verdict, blockers=blockers))
                try:
                    result, sidecar, outputs = harness.run()
                finally:
                    harness.close()
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(sidecar["classification"], "valid_verdict")
                self.assertEqual(sidecar["verdict"], verdict)
                self.assertEqual(sidecar["reviewed_head_sha"], HEAD)
                self.assertEqual(sidecar["review_workflow_run_id"], 91234)
                self.assertEqual(outputs["classification"], "valid_verdict")
                self.assertNotIn("summary", outputs)

    def test_pass_may_carry_deferred_notes_without_blockers(self):
        harness = ValidatorHarness(review_payload(
            "PASS",
            blockers=[],
            major_notes=["optional rename residual"],
            minor_notes=["comment polish deferred"],
        ))
        try:
            result, sidecar, outputs = harness.run()
        finally:
            harness.close()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sidecar["verdict"], "PASS")
        self.assertEqual(sidecar["major_notes"], ["optional rename residual"])
        self.assertEqual(sidecar["minor_notes"], ["comment polish deferred"])
        self.assertEqual(outputs["verdict"], "PASS")

    def test_invalid_results_fail_and_leave_only_bounded_failure_metadata(self):
        cases = {
            "pass_blockers": (review_payload("PASS", blockers=["no"]), None, True, "pass_has_blockers"),
            "pass_missing_proof": (review_payload("PASS", security_ok=False), None, True, "pass_proof_missing"),
            "pass_with_notes_has_blockers": (
                review_payload("PASS_WITH_NOTES", blockers=["no"]), None, True, "pass_with_notes_has_blockers"
            ),
            "blocked_without_blockers": (
                review_payload("BLOCKED", blockers=[]), None, True, "blocked_without_blockers"
            ),
            "malformed_json": (None, b"{not json", True, "artifact_invalid_json"),
            "missing": (None, None, False, "artifact_missing"),
            "oversized": (None, b"x" * (65 * 1024), True, "artifact_too_large"),
            "head_mismatch": (review_payload("BLOCKED", reviewed_head_sha=OLD_HEAD), None, True, "reviewed_head_mismatch"),
        }
        for name, (payload, raw, exists, failure_code) in cases.items():
            with self.subTest(name=name):
                harness = ValidatorHarness(payload, raw, exists)
                try:
                    result, sidecar, outputs = harness.run()
                finally:
                    harness.close()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(sidecar["classification"], "invalid_artifact")
                self.assertEqual(sidecar["failure_code"], failure_code)
                self.assertEqual(outputs["verdict"], "INVALID")
                self.assertNotIn("summary", outputs)

    def test_newline_and_delimiter_text_cannot_inject_workflow_outputs(self):
        harness = ValidatorHarness(review_payload(
            "BLOCKED",
            summary="line one\nagent_output_deadbeef\nverdict=PASS",
            blockers=["line two\nclassification=valid_verdict"],
        ))
        try:
            result, sidecar, outputs = harness.run()
        finally:
            harness.close()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(outputs["verdict"], "BLOCKED")
        self.assertNotIn("summary", outputs)
        self.assertIn("verdict=PASS", sidecar["summary"])


class TestReviewStateFinalization(unittest.TestCase):
    def _write_fake_gh(self, root, state_path):
        fake = root / "gh"
        fake.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "path = os.environ['REVIEW_GH_STATE']\n"
            "state = json.load(open(path))\n"
            "args = sys.argv[1:]\n"
            "def save(): json.dump(state, open(path, 'w'))\n"
            "if args[:2] == ['pr', 'view']:\n"
            " print(json.dumps(state['pr']))\n"
            "elif args[:2] == ['pr', 'list']:\n"
            " print('[]')\n"
            "elif args[:2] == ['issue', 'view']:\n"
            " field = args[args.index('--json') + 1]\n"
            " if field == 'comments': print(json.dumps({'comments': state['comments']}))\n"
            " elif field == 'labels': print(json.dumps({'labels': [{'name': value} for value in state['labels']]}))\n"
            " else: raise SystemExit('unexpected issue view field')\n"
            "elif args[:2] == ['issue', 'comment']:\n"
            " state['comments'].append({'author': {'login': 'github-actions'}, 'body': args[args.index('--body') + 1]}); save()\n"
            "elif args[:2] == ['issue', 'edit']:\n"
            " labels = set(state['labels'])\n"
            " index = 0\n"
            " while index < len(args):\n"
            "  if args[index] == '--add-label': labels.update(args[index + 1].split(',')); index += 2; continue\n"
            "  if args[index] == '--remove-label': labels.discard(args[index + 1]); index += 2; continue\n"
            "  index += 1\n"
            " state['labels'] = sorted(labels); save()\n"
            "else:\n"
            " raise SystemExit('unexpected gh args: ' + repr(args))\n"
        )
        fake.chmod(fake.stat().st_mode | stat.S_IXUSR)
        return fake

    def test_workflow_facing_record_command_persists_nonpass_and_releases_review_capacity(self):
        harness = ValidatorHarness(review_payload("BLOCKED", blockers=["exact blocker"]))
        try:
            validator, sidecar, _ = harness.run()
            self.assertEqual(validator.returncode, 0, validator.stderr)
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                state_path = root / "state.json"
                worker = {
                    "kind": "agent-orchestrator-state", "version": 1,
                    "pr_number": 207, "head_sha": HEAD, "worker_type": "implementation",
                    "extra": {"branch": "agent/issue-42"},
                }
                state_path.write_text(json.dumps({
                    "labels": ["review-running"],
                    "comments": [{"author": {"login": "github-actions"}, "body": json.dumps(worker)}],
                    "pr": {
                        "state": "OPEN", "headRefName": "agent/issue-42", "headRefOid": HEAD,
                        "baseRefName": "main",
                        "body": "Closes #42\n<!-- agent-orchestrator-binding: {\"issue_number\": 42, \"branch\": \"agent/issue-42\"} -->",
                    },
                }))
                fake = self._write_fake_gh(root, state_path)
                env = {
                    **os.environ, "AGENT_GH_CMD": str(fake), "AGENT_REPO": "acme/repo",
                    "REVIEW_GH_STATE": str(state_path),
                }
                command = [
                    sys.executable, str(CONTROL / "state_manager.py"), "record-review", "42", "207", HEAD,
                    "--evidence-file", str(harness.sidecar),
                ]
                first = subprocess.run(command, cwd=ROOT, env=env, text=True, capture_output=True)
                second = subprocess.run(command, cwd=ROOT, env=env, text=True, capture_output=True)
                labels = subprocess.run(
                    [sys.executable, str(CONTROL / "state_manager.py"), "finalize-review-labels", "42", "BLOCKED"],
                    cwd=ROOT, env=env, text=True, capture_output=True,
                )
                state = json.loads(state_path.read_text())
        finally:
            harness.close()
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(labels.returncode, 0, labels.stderr)
        durable = [json.loads(item["body"]) for item in state["comments"] if "review-state" in item["body"]]
        self.assertEqual(len(durable), 1)
        self.assertEqual(durable[0]["verdict"], "BLOCKED")
        self.assertEqual(durable[0]["blockers"], ["exact blocker"])
        self.assertEqual(durable[0]["reviewed_head_sha"] if "reviewed_head_sha" in durable[0] else durable[0]["head_sha"], HEAD)
        self.assertEqual(state["labels"], ["agent-review-blocked"])

    def test_stale_and_invalid_results_cannot_overwrite_current_pass_or_claim(self):
        valid = tempfile.NamedTemporaryFile(mode="w", delete=False)
        try:
            json.dump({
                "kind": "agent-orchestrator-review-validation", "version": 1,
                "classification": "valid_verdict", "pr_number": 207,
                "reviewed_head_sha": HEAD, "verdict": "FAIL", "summary": "stale",
                "blockers": [], "major_notes": [], "minor_notes": [],
                "artifact_sha256": "c" * 64, "review_workflow_run_id": 9,
            }, valid)
            valid.close()
            with mock.patch.object(sm, "verify_issue_pr_binding", return_value=(False, "head_mismatch")), \
                 mock.patch.object(sm, "record_review_state") as record:
                ok, reason = sm.record_validated_review(42, 207, HEAD, valid.name, "acme/repo")
            self.assertFalse(ok)
            self.assertIn("binding_rejected", reason)
            record.assert_not_called()

            invalid = tempfile.NamedTemporaryFile(mode="w", delete=False)
            json.dump({
                "kind": "agent-orchestrator-review-validation", "version": 1,
                "classification": "invalid_artifact", "pr_number": 207,
                "reviewed_head_sha": HEAD, "failure_code": "artifact_invalid_json",
                "artifact_sha256": None, "review_workflow_run_id": 9,
            }, invalid)
            invalid.close()
            with mock.patch.object(sm, "verify_issue_pr_binding", return_value=(True, "ok")), \
                 mock.patch.object(sm, "read_review_state", return_value={"pr_number": 207, "head_sha": HEAD, "verdict": "PASS"}), \
                 mock.patch.object(sm, "comment_on_issue") as comment:
                ok, reason = sm.record_review_validation_failure(42, 207, HEAD, invalid.name, "acme/repo")
            self.assertTrue(ok, reason)
            self.assertEqual(reason, "newer_or_current_review_state_exists")
            comment.assert_not_called()
        finally:
            os.unlink(valid.name)
            os.unlink(invalid.name)

    def test_failure_release_does_not_replace_a_newer_pass_state(self):
        with mock.patch.object(sm, "get_issue_labels_checked", return_value={sm.LABEL_REVIEW_PASSED, sm.LABEL_MERGE_READY}), \
             mock.patch.object(sm, "set_labels") as transition:
            ok, reason = sm.release_failed_capacity(
                42, sm.LABEL_REVIEW_RUNNING, sm.LABEL_REVIEW_BLOCKED, HEAD, "acme/repo"
            )
        self.assertFalse(ok)
        self.assertEqual(reason, "active_state_mismatch")
        transition.assert_not_called()


def review_node(review_id, state, author="reviewer", head=HEAD, submitted="2026-07-14T00:00:00Z"):
    return {
        "id": review_id,
        "state": state,
        "submittedAt": submitted,
        "author": {"id": f"USER_{author}", "login": author, "__typename": "User"},
        "commit": {"oid": head},
    }


def review_page(nodes, decision=None, has_next=False, cursor=None, head=HEAD):
    return json.dumps({"data": {"repository": {"pullRequest": {
        "headRefOid": head,
        "reviewDecision": decision,
        "reviews": {"nodes": nodes, "pageInfo": {"hasNextPage": has_next, "endCursor": cursor}},
    }}}})


def thread_page(nodes, has_next=False, cursor=None, include_page_info=True, head=HEAD):
    threads = {"nodes": nodes}
    if include_page_info:
        threads["pageInfo"] = {"hasNextPage": has_next, "endCursor": cursor}
    return json.dumps({"data": {"repository": {"pullRequest": {
        "headRefOid": head,
        "reviewThreads": threads,
    }}}})


class TestCurrentEffectiveReviews(unittest.TestCase):
    def test_review_query_requests_stable_user_id_through_actor_fragment(self):
        with mock.patch.object(sm, "_gh", return_value="{}") as gh:
            sm._graphql_review_page("acme", "repo", 207)
        query = next(value.split("=", 1)[1] for value in gh.call_args.args if value.startswith("query="))
        self.assertIn("author{login __typename ... on User{id}}", query)
        self.assertNotIn("author{id", query)

    def _effective(self, nodes, decision=None):
        with mock.patch.object(sm, "_gh", return_value=review_page(nodes, decision)):
            return sm.current_effective_reviews(207, HEAD, "acme/repo")

    def test_latest_effective_review_per_human_reviewer(self):
        cases = (
            ("current changes", [review_node("r1", "CHANGES_REQUESTED")], "CHANGES_REQUESTED", ["r1"]),
            ("changes then approval", [review_node("r1", "CHANGES_REQUESTED"), review_node("r2", "APPROVED", submitted="2026-07-14T00:01:00Z")], "APPROVED", []),
            ("approval then changes", [review_node("r1", "APPROVED"), review_node("r2", "CHANGES_REQUESTED", submitted="2026-07-14T00:01:00Z")], "CHANGES_REQUESTED", ["r2"]),
            ("dismissed changes", [review_node("r1", "CHANGES_REQUESTED"), review_node("r2", "DISMISSED", submitted="2026-07-14T00:01:00Z")], None, []),
            ("old changes current approval", [review_node("r1", "CHANGES_REQUESTED", head=OLD_HEAD), review_node("r2", "APPROVED", submitted="2026-07-14T00:01:00Z")], "APPROVED", []),
            ("old changes no replacement", [review_node("r1", "CHANGES_REQUESTED", head=OLD_HEAD)], None, ["r1"]),
            ("two reviewers one changes", [review_node("r1", "APPROVED", author="one"), review_node("r2", "CHANGES_REQUESTED", author="two")], "CHANGES_REQUESTED", ["r2"]),
            ("duplicate event", [review_node("r1", "APPROVED"), review_node("r1", "APPROVED")], "APPROVED", []),
        )
        for name, nodes, decision, expected in cases:
            with self.subTest(name=name):
                effective = self._effective(nodes, decision)
                self.assertEqual(effective["requested_change_review_ids"], expected)

    def test_malformed_unavailable_and_contradictory_reviews_fail_closed(self):
        malformed = review_node("r1", "CHANGES_REQUESTED")
        malformed["author"] = None
        cases = (
            [review_page(malformed)],
            [None],
            [review_page([review_node("r1", "CHANGES_REQUESTED")], "APPROVED")],
            [review_page([], "CHANGES_REQUESTED")],
        )
        for responses in cases:
            with self.subTest(responses=responses), mock.patch.object(sm, "_gh", side_effect=responses):
                with self.assertRaises(sm.StateUnavailableError):
                    sm.current_effective_reviews(207, HEAD, "acme/repo")

    def test_review_nodes_are_paginated_before_effective_state_is_selected(self):
        pages = [
            review_page([review_node("old", "CHANGES_REQUESTED", head=OLD_HEAD)], None, True, "next"),
            review_page([review_node("new", "APPROVED", submitted="2026-07-14T00:01:00Z")], None),
        ]
        with mock.patch.object(sm, "_gh", side_effect=pages):
            effective = sm.current_effective_reviews(207, HEAD, "acme/repo")
        self.assertEqual(effective["pages"], 2)
        self.assertEqual(effective["requested_changes"], [])
        with mock.patch.object(sm, "_gh", side_effect=[
            review_page([], None, True, "repeat"),
            review_page([], None, True, "repeat"),
        ]):
            with self.assertRaises(sm.StateUnavailableError):
                sm.current_effective_reviews(207, HEAD, "acme/repo")

    def test_no_reviews_without_a_repository_approval_rule_does_not_block(self):
        effective = self._effective([], None)
        self.assertEqual(effective["requested_changes"], [])
        self.assertIsNone(effective["review_decision"])

    def test_authoritative_review_required_decision_blocks_pending_required_review(self):
        effective = self._effective([], "REVIEW_REQUIRED")
        self.assertEqual(effective["review_decision"], "REVIEW_REQUIRED")


class TestReviewThreadPagination(unittest.TestCase):
    def _status(self, responses):
        with mock.patch.object(sm, "_gh", side_effect=responses):
            return sm.review_threads_status(207, HEAD, "acme/repo")

    def test_complete_pagination_including_page_two_unresolved_thread(self):
        first = [{"id": f"t{index}", "isResolved": True} for index in range(100)]
        first[99] = {"id": "t99", "isResolved": False}
        second = [{"id": "t100", "isResolved": False}]
        status = self._status([thread_page(first, True, "cursor-1"), thread_page(second)])
        self.assertTrue(status["complete"])
        self.assertEqual(status["total_threads"], 101)
        self.assertEqual(status["unresolved_thread_ids"], ["t99", "t100"])
        self.assertEqual(status["pages"], 2)

    def test_zero_less_than_and_exactly_one_hundred_threads(self):
        for count in (0, 3, 100):
            with self.subTest(count=count):
                nodes = [{"id": f"t{index}", "isResolved": index % 2 == 0} for index in range(count)]
                status = self._status([thread_page(nodes)])
                self.assertEqual(status["total_threads"], count)
                self.assertEqual(status["unresolved_thread_ids"], [f"t{index}" for index in range(count) if index % 2])

    def test_partial_or_malformed_thread_results_never_authorize(self):
        cases = (
            [thread_page([], include_page_info=False)],
            [thread_page([], True, None)],
            [thread_page([], True, "again"), thread_page([], True, "again")],
            [thread_page([{"id": "bad", "isResolved": "false"}])],
            [thread_page([], True, "cursor"), None],
            [thread_page([{"id": "same", "isResolved": True}], True, "cursor"), thread_page([{"id": "same", "isResolved": True}])],
        )
        for responses in cases:
            with self.subTest(responses=responses):
                with self.assertRaises(sm.StateUnavailableError):
                    self._status(responses)
        with mock.patch.object(sm, "MAX_REVIEW_THREADS", 1):
            with self.assertRaises(sm.StateUnavailableError):
                self._status([thread_page([{"id": "one", "isResolved": True}, {"id": "two", "isResolved": True}])])
        with mock.patch.object(sm, "MAX_REVIEW_THREAD_PAGES", 1):
            with self.assertRaises(sm.StateUnavailableError):
                self._status([thread_page([], True, "cursor")])
        with self.assertRaises(sm.StateUnavailableError):
            self._status([thread_page([], head=OLD_HEAD)])

    def test_merge_verifier_rejects_incomplete_threads_and_effective_requested_changes(self):
        base_pr = {"state": "OPEN", "baseRefName": "main", "headRefOid": HEAD, "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN"}
        with mock.patch.object(sm, "get_issue_labels", return_value={sm.LABEL_REVIEW_PASSED, sm.LABEL_MERGE_READY}), \
             mock.patch.object(sm, "get_pr_info", return_value=base_pr), \
             mock.patch.object(sm, "verify_issue_pr_binding", return_value=(True, "ok")), \
             mock.patch.object(sm, "read_review_state", return_value={"pr_number": 207, "head_sha": HEAD, "verdict": "PASS"}), \
             mock.patch.object(sm, "current_effective_reviews", return_value={"review_decision": None, "requested_changes": []}), \
             mock.patch.object(sm, "review_threads_status", return_value={"complete": False, "unresolved_thread_ids": []}), \
             mock.patch.object(sm, "read_ci_state"), \
             mock.patch("control_state.require_auto_merge", return_value={}):
            with self.assertRaisesRegex(RuntimeError, "pagination is incomplete"):
                sm.verify_merge_requirements(207, 42, HEAD, "acme/repo")
        with mock.patch.object(sm, "get_issue_labels", return_value={sm.LABEL_REVIEW_PASSED, sm.LABEL_MERGE_READY}), \
             mock.patch.object(sm, "get_pr_info", return_value=base_pr), \
             mock.patch.object(sm, "verify_issue_pr_binding", return_value=(True, "ok")), \
             mock.patch.object(sm, "read_review_state", return_value={"pr_number": 207, "head_sha": HEAD, "verdict": "PASS"}), \
             mock.patch.object(sm, "current_effective_reviews", return_value={"review_decision": "CHANGES_REQUESTED", "requested_changes": [{"id": "r1"}]}), \
             mock.patch("control_state.require_auto_merge", return_value={}):
            with self.assertRaisesRegex(RuntimeError, "requested-changes"):
                sm.verify_merge_requirements(207, 42, HEAD, "acme/repo")
        with mock.patch.object(sm, "get_issue_labels", return_value={sm.LABEL_REVIEW_PASSED, sm.LABEL_MERGE_READY}), \
             mock.patch.object(sm, "get_pr_info", return_value=base_pr), \
             mock.patch.object(sm, "verify_issue_pr_binding", return_value=(True, "ok")), \
             mock.patch.object(sm, "read_review_state", return_value={"pr_number": 207, "head_sha": HEAD, "verdict": "PASS"}), \
             mock.patch.object(sm, "current_effective_reviews", return_value={"review_decision": "REVIEW_REQUIRED", "requested_changes": []}), \
             mock.patch("control_state.require_auto_merge", return_value={}):
            with self.assertRaisesRegex(RuntimeError, "requires review"):
                sm.verify_merge_requirements(207, 42, HEAD, "acme/repo")


if __name__ == "__main__":
    unittest.main()
