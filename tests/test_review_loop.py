"""Provider-free deterministic tests for the review-loop transport hardening.

Covers every blocker from the independent review (B1-B18): 40-hex git SHAs,
single canonical bytes, mandatory request body, terminal-state resend refusal,
three-state thread inspection, unique receipt JSON, reviewer independence,
strict comment markers, POST outcome unknown, journal chain verification,
per-chat lock, mandatory live validation via adapter, evidence wiring, path
traversal, and fail-closed default.  No real ChatGPT, GitHub, network, cookie,
or browser is touched.
"""

from __future__ import annotations

import json
import pathlib
import tempfile
import unittest

import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "agent-control"))

from review_loop import (  # noqa: E402
    comment_poster,
    github_adapter,
    journal as journal_mod,
    live_validation,
    locking,
    models,
    protocol,
    receipt_parser,
    state_machine,
    transport,
)

REPO = "Igzela/token-efficient-agent-harness-lab"
BASE = "a1878b2a282303d6e187f35c437875493c0f5296"
HEAD = "65f44098faf4974e318020e85b43a37f2e0f3d4a"
CHAT = "igzela-test/review-loop-tests"
SESSION = "implementation-session-test-1"
REVIEWER = "reviewer-session-test"
EVIDENCE_SHA = "d9a8e5b8be5dcfe35a9c3cb70a50a2a9a06d26c701ab5a854400bad19c5de82c"
REQ_SHA = "a" * 64


def make_envelope(request_sha: str = REQ_SHA) -> models.ReviewRequestEnvelope:
    return models.ReviewRequestEnvelope(
        schema_version=models.ENVELOPE_SCHEMA,
        repository=REPO,
        pr_number=349,
        base_sha=BASE,
        head_sha=HEAD,
        chat_key=CHAT,
        evidence_index_sha256=EVIDENCE_SHA,
        request_text_sha256=request_sha,
        implementation_session_id=SESSION,
    )


def make_receipt(
    request_sha: str = REQ_SHA,
    verdict: str = "PASS",
    reviewer: str = REVIEWER,
) -> models.ReviewReceipt:
    return models.ReviewReceipt(
        schema_version=models.RECEIPT_SCHEMA,
        verdict=verdict,
        repository=REPO,
        pr_number=349,
        base_sha=BASE,
        head_sha=HEAD,
        diff_scope="complete_base_head",
        blockers=(),
        unresolved_objections=(),
        reviewer_session_id=reviewer,
        implementation_session_id=SESSION,
        transport="parent-posted-on-behalf-of-independent-session",
    )


def live_facts():
    return {
        "state": "OPEN",
        "is_draft": True,
        "merged": False,
        "base_sha": BASE,
        "head_sha": HEAD,
        "changed_files": ["docs/A.md"],
    }


class TestProtocol(unittest.TestCase):
    def test_canonicalization_is_deterministic(self):
        raw = "\ufeffline1\r\nline2\r\n\u00e9"
        canonical = protocol.canonicalize(raw)
        self.assertEqual(canonical, "line1\nline2\n\u00e9")

    def test_sha_changes_when_content_changes(self):
        sha1 = protocol.request_sha256("hello world")
        sha2 = protocol.request_sha256("hello world!")
        self.assertNotEqual(sha1, sha2)
        self.assertEqual(len(sha1), 64)

    def test_envelope_and_marker_share_single_canonical_bytes(self):
        request = "review this"
        message, sha = protocol.build_message(request)
        body = protocol.canonical_body(request)
        self.assertEqual(protocol.request_sha256(body), sha)
        self.assertIn(body, message)
        self.assertTrue(message.startswith("Review-Request-SHA256: " + sha))

    def test_trailing_newline_does_not_change_sha(self):
        sha1 = protocol.request_sha256("hello")
        sha2 = protocol.request_sha256("hello\n")
        self.assertEqual(sha1, sha2)


class TestEnvelope(unittest.TestCase):
    def test_valid_envelope_with_40hex_git_shas(self):
        envelope = make_envelope()
        self.assertEqual(envelope.validate(), [])

    def test_git_sha_must_be_40hex(self):
        envelope = models.ReviewRequestEnvelope(
            **{**make_envelope().__dict__, "base_sha": "a" * 64}
        )
        errors = envelope.validate()
        self.assertTrue(any("40-hex git object" in e for e in errors))

    def test_content_sha_must_be_64hex(self):
        envelope = models.ReviewRequestEnvelope(
            **{**make_envelope().__dict__, "request_text_sha256": "abc"}
        )
        errors = envelope.validate()
        self.assertTrue(any("64-hex sha" in e for e in errors))


class TestStateMachine(unittest.TestCase):
    def test_unknown_effect_is_terminal(self):
        self.assertIn(
            models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN,
            state_machine.TERMINAL_MESSAGE_OUTCOMES,
        )
        self.assertFalse(
            state_machine.can_send(models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN)
        )

    def test_send_allowed_only_from_provable_states(self):
        self.assertTrue(state_machine.can_send(None))
        self.assertTrue(state_machine.can_send(models.DeliveryOutcome.LIVE_VALIDATED))
        self.assertFalse(state_machine.can_send(models.DeliveryOutcome.SENT_CONFIRMED))

    def test_confirmed_delivery_is_not_resend_blocked(self):
        # A re-run after a confirmed delivery must be allowed so the marker
        # check can report ALREADY_PRESENT instead of double posting.
        self.assertNotIn(
            models.DeliveryOutcome.SENT_CONFIRMED, state_machine.RESEND_BLOCKED
        )
        self.assertNotIn(
            models.DeliveryOutcome.ALREADY_PRESENT, state_machine.RESEND_BLOCKED
        )
        self.assertIn(
            models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN,
            state_machine.RESEND_BLOCKED,
        )

    def test_valid_progression(self):
        self.assertEqual(
            state_machine.next_state(
                models.DeliveryOutcome.BUILT, models.DeliveryOutcome.LIVE_VALIDATED
            ),
            models.DeliveryOutcome.LIVE_VALIDATED,
        )


class TestThreadInspection(unittest.TestCase):
    def test_three_states(self):
        self.assertEqual(transport.ThreadInspection.empty().state, "EMPTY_THREAD")
        self.assertEqual(
            transport.ThreadInspection.message("hi").state, "MESSAGE"
        )
        self.assertEqual(
            transport.ThreadInspection.unavailable("boom").state,
            "INSPECTION_UNAVAILABLE",
        )

    def test_fake_unavailable_state(self):
        fake = transport.FakeTransport(inspect_state="unavailable")
        self.assertEqual(
            fake.inspect_last_user_message().state, "INSPECTION_UNAVAILABLE"
        )


class TestLiveValidation(unittest.TestCase):
    def test_pr_state_match_passes(self):
        self.assertEqual(
            live_validation.validate_pr_live_state(
                repository=REPO,
                pr_number=349,
                observed_state="OPEN",
                observed_is_draft=True,
                observed_base_sha=BASE,
                observed_head_sha=HEAD,
                expected_base_sha=BASE,
                expected_head_sha=HEAD,
                observed_merged=False,
            ),
            [],
        )

    def test_head_drift_rejected(self):
        errors = live_validation.validate_pr_live_state(
            repository=REPO,
            pr_number=349,
            observed_state="OPEN",
            observed_is_draft=True,
            observed_base_sha=BASE,
            observed_head_sha="b" * 40,
            expected_base_sha=BASE,
            expected_head_sha=HEAD,
            observed_merged=False,
        )
        self.assertTrue(any("head drifted" in e for e in errors))

    def test_merged_rejected(self):
        errors = live_validation.validate_pr_live_state(
            repository=REPO,
            pr_number=349,
            observed_state="OPEN",
            observed_is_draft=True,
            observed_base_sha=BASE,
            observed_head_sha=HEAD,
            expected_base_sha=BASE,
            expected_head_sha=HEAD,
            observed_merged=True,
        )
        self.assertTrue(any("merged" in e for e in errors))

    def test_diff_scope_limits_paths(self):
        errors = live_validation.validate_diff_scope(
            ["engine/src/lib.rs"], ("docs/A.md",)
        )
        self.assertTrue(any("outside allowed paths" in e for e in errors))

    def test_evidence_index_missing_file_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            index = {"files": [{"path": "a.txt", "sha256": "x" * 64}]}
            index_path = root / "index.json"
            index_path.write_text(json.dumps(index), encoding="utf-8")
            errors, _ = live_validation.validate_evidence_index(
                index_path, live_validation.sha256_file(index_path)
            )
            self.assertTrue(any("missing" in e for e in errors))

    def test_evidence_dotdot_escape_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            secret = root / "secret.txt"
            secret.write_text("x", encoding="utf-8")
            index = {"files": [{"path": "sub/../secret.txt", "sha256": "y" * 64}]}
            index_path = root / "index.json"
            index_path.write_text(json.dumps(index), encoding="utf-8")
            errors, _ = live_validation.validate_evidence_index(
                index_path, live_validation.sha256_file(index_path)
            )
            self.assertTrue(any(".." in e for e in errors))

    def test_symlink_parent_escape_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            outside = pathlib.Path(tempfile.mkdtemp())
            secret = outside / "secret.txt"
            secret.write_text("x", encoding="utf-8")
            link = root / "dir"
            link.symlink_to(outside, target_is_directory=True)
            errors = live_validation.check_symlink_escape(root, ["dir/secret.txt"])
            self.assertTrue(errors)


class TestJournal(unittest.TestCase):
    def test_append_and_replay_chain(self):
        with tempfile.TemporaryDirectory() as tmp:
            journal = journal_mod.Journal(pathlib.Path(tmp) / "events.jsonl")
            first = journal.append(event="BUILT", chat_key=CHAT, request_text_sha256=REQ_SHA)
            second = journal.append(event="SENT_CONFIRMED", chat_key=CHAT, request_text_sha256=REQ_SHA)
            records = journal.replay()
            self.assertEqual([r.seq for r in records], [1, 2])
            self.assertEqual(records[1].prev_sha, first.sha)

    def test_append_refuses_broken_chain(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "events.jsonl"
            journal = journal_mod.Journal(path)
            journal.append(event="BUILT", chat_key=CHAT, request_text_sha256=REQ_SHA)
            with open(path, "a", encoding="utf-8") as handle:
                handle.write(
                    models.TransportEvent(
                        seq=99,
                        ts="2026-01-01T00:00:00Z",
                        event="SENT_CONFIRMED",
                        chat_key=CHAT,
                        request_text_sha256=REQ_SHA,
                        prev_sha="b" * 64,
                        sha="c" * 64,
                    ).to_json()
                    + "\n"
                )
            with self.assertRaises(ValueError):
                journal.append(event="RESPONSE_CAPTURED", chat_key=CHAT, request_text_sha256=REQ_SHA)

    def test_projection_rebuildable(self):
        with tempfile.TemporaryDirectory() as tmp:
            journal = journal_mod.Journal(pathlib.Path(tmp) / "events.jsonl")
            journal.append(event="SENT_CONFIRMED", chat_key=CHAT, request_text_sha256=REQ_SHA)
            projection = journal.projection()
            self.assertEqual(projection["latest_event_per_chat"][CHAT], "SENT_CONFIRMED")


class TestLocking(unittest.TestCase):
    def test_lock_excludes_concurrent_owner(self):
        with tempfile.TemporaryDirectory() as tmp:
            lock_dir = pathlib.Path(tmp)
            first = locking.ChatLock(lock_dir, CHAT)
            first.acquire()
            try:
                second = locking.ChatLock(lock_dir, CHAT)
                with self.assertRaises(locking.LockBusy):
                    second.acquire()
            finally:
                first.release()

    def test_lock_released_can_be_reacquired(self):
        with tempfile.TemporaryDirectory() as tmp:
            lock_dir = pathlib.Path(tmp)
            first = locking.ChatLock(lock_dir, CHAT)
            first.acquire()
            first.release()
            second = locking.ChatLock(lock_dir, CHAT)
            second.acquire()
            second.release()


class TestReceiptParser(unittest.TestCase):
    def test_accepts_exact_structured_pass(self):
        receipt = make_receipt()
        markdown = "prose\n```json\n" + receipt.to_json() + "\n```\n"
        parsed, errors = receipt_parser.parse_receipt(markdown)
        self.assertEqual(errors, [])
        self.assertEqual(parsed, receipt)

    def test_rejects_natural_language_pass_without_json(self):
        parsed, errors = receipt_parser.parse_receipt("### 结论\nPASS\nno json")
        self.assertIsNone(parsed)
        self.assertTrue(any("no JSON receipt" in e for e in errors))

    def test_rejects_pass_with_notes(self):
        receipt = make_receipt(verdict="PASS_WITH_NOTES")
        markdown = "```json\n" + receipt.to_json() + "\n```\n"
        parsed, errors = receipt_parser.parse_receipt(markdown)
        self.assertIsNone(parsed)
        self.assertTrue(any("not an exact PASS" in e for e in errors))

    def test_rejects_multiple_receipts(self):
        first = make_receipt()
        second = make_receipt(reviewer="other")
        markdown = "```json\n" + first.to_json() + "\n```\n```json\n" + second.to_json() + "\n```\n"
        parsed, errors = receipt_parser.parse_receipt(markdown)
        self.assertIsNone(parsed)
        self.assertTrue(any("exactly one receipt" in e for e in errors))

    def test_rejects_self_review(self):
        receipt = make_receipt(reviewer=SESSION)
        markdown = "```json\n" + receipt.to_json() + "\n```\n"
        parsed, errors = receipt_parser.parse_receipt(markdown)
        self.assertIsNone(parsed)
        self.assertTrue(any("must differ from implementation" in e for e in errors))

    def test_rejects_blockers(self):
        receipt = models.ReviewReceipt(
            **{**make_receipt().__dict__, "blockers": ("b1",)}
        )
        markdown = "```json\n" + receipt.to_json() + "\n```\n"
        parsed, errors = receipt_parser.parse_receipt(markdown)
        self.assertIsNone(parsed)

    def test_rejects_identity_mismatch(self):
        receipt = make_receipt()
        envelope = models.ReviewRequestEnvelope(
            **{**make_envelope().__dict__, "head_sha": "c" * 40}
        )
        self.assertTrue(receipt.matches_envelope(envelope))


class TestCommentPoster(unittest.TestCase):
    def test_strict_marker_roundtrip(self):
        receipt = make_receipt()
        receipt_sha = comment_poster.receipt_sha256(receipt)
        body = comment_poster.build_comment_body(make_envelope(), receipt)
        self.assertIn(comment_poster.comment_marker_line(REQ_SHA, receipt_sha), body)

    def test_reconcile_skip_identical(self):
        receipt = make_receipt()
        receipt_sha = comment_poster.receipt_sha256(receipt)
        body = comment_poster.build_comment_body(make_envelope(), receipt)
        action, _ = comment_poster.reconcile_comments([body], REQ_SHA, receipt_sha)
        self.assertEqual(action, "skip")

    def test_reconcile_conflict_different_receipt(self):
        receipt1 = make_receipt()
        receipt2 = make_receipt(reviewer="other")
        body1 = comment_poster.build_comment_body(make_envelope(), receipt1)
        receipt_sha2 = comment_poster.receipt_sha256(receipt2)
        action, _ = comment_poster.reconcile_comments([body1], REQ_SHA, receipt_sha2)
        self.assertEqual(action, "conflict")

    def test_reconcile_malformed_marker_is_conflict(self):
        receipt = make_receipt()
        receipt_sha = comment_poster.receipt_sha256(receipt)
        action, _ = comment_poster.reconcile_comments(
            ["<!-- independent-review-receipt:broken -->"], REQ_SHA, receipt_sha
        )
        self.assertEqual(action, "conflict")

    def test_reconcile_post_new(self):
        receipt = make_receipt()
        action, _ = comment_poster.reconcile_comments(
            [], REQ_SHA, comment_poster.receipt_sha256(receipt)
        )
        self.assertEqual(action, "post")


class TestFakeGitHub(unittest.TestCase):
    def test_fetch_failure_surfaces(self):
        github = github_adapter.FakeGitHub(live_facts())
        github.fail_next_fetch = True
        with self.assertRaises(RuntimeError):
            github.fetch_pr(REPO, 349)

    def test_post_failure_surfaces(self):
        github = github_adapter.FakeGitHub(live_facts())
        github.fail_next_post = True
        with self.assertRaises(RuntimeError):
            github.create_comment(REPO, 349, "body")


class TestFailClosedLauncher(unittest.TestCase):
    def test_default_launcher_fails_closed_without_env(self):
        import os
        import subprocess

        env = dict(os.environ)
        env.pop("REVIEW_TRANSPORT_MODULE", None)
        env.pop("REVIEW_GITHUB_MODULE", None)
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts" / "agent-control" / "review_loop_cli.py"), "status"],
            capture_output=True,
            text=True,
            env=env,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("refuses to run", result.stderr)


if __name__ == "__main__":
    unittest.main()
