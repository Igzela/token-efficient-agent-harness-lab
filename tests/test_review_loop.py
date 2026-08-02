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
                observed_repository=REPO,
                observed_pr_number=349,
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
            observed_repository=REPO,
            observed_pr_number=349,
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
            observed_repository=REPO,
            observed_pr_number=349,
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


def make_evidence_index(root: pathlib.Path) -> pathlib.Path:
    """Create a valid evidence index with one hashed file; return its path."""
    target = root / "evidence.txt"
    target.write_text("evidence-content", encoding="utf-8")
    digest = live_validation.sha256_file(target)
    index = {"files": [{"path": "evidence.txt", "sha256": digest}]}
    index_path = root / "index.json"
    index_path.write_text(json.dumps(index), encoding="utf-8")
    return index_path


def build_envelope(tmp: pathlib.Path, chat_key: str = CHAT) -> pathlib.Path:
    """Run the build subcommand end to end; return the envelope path."""
    from review_loop import cli

    request_path = tmp / "request.md"
    request_path.write_text("Independent review request body.\n", encoding="utf-8")
    index = make_evidence_index(tmp)
    env_path = tmp / "env.json"
    cli.main(
        [
            "build",
            "--request", str(request_path),
            "--repository", REPO,
            "--pr", "349",
            "--base-sha", BASE,
            "--head-sha", HEAD,
            "--chat-key", chat_key,
            "--evidence-index", str(index),
            "--evidence-index-sha256", live_validation.sha256_file(index),
            "--session-id", SESSION,
            "--out", str(env_path),
            "--message-out", str(tmp / "message.txt"),
            "--body-out", str(tmp / "body.txt"),
        ]
    )
    return env_path


def run_send(tmp: pathlib.Path, transport=None, github=None, journal=None, chat_key: str = CHAT):
    from review_loop import cli

    return cli.main(
        ["send", "--envelope", str(tmp / "env.json"), "--body", str(tmp / "body.txt"),
         "--evidence-index", str(tmp / "index.json"), "--allowed-paths", "docs/A.md"],
        transport=transport or transport.FakeTransport(),
        github=github or github_adapter.FakeGitHub(live_facts()),
        journal=journal,
        lock_dir=tmp / "locks",
    )


def env_keys(tmp: pathlib.Path) -> tuple[str, str]:
    """Return (chat_key, request_text_sha256) bound to the built envelope."""
    data = json.loads((tmp / "env.json").read_text(encoding="utf-8"))
    return data["chat_key"], data["request_text_sha256"]


class TestReconciliation(unittest.TestCase):
    """R2-B1: outcome-unknown has a read-only reconciliation path."""

    def test_unknown_then_reconcile_empty_thread_resends(self):
        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            build_envelope(tmp)
            journal = journal_mod.Journal(tmp / "events.jsonl")

            t1 = transport.FakeTransport()
            t1.send_failure = RuntimeError("network dropped")
            with self.assertRaises(SystemExit):
                run_send(tmp, transport=t1, journal=journal)
            self.assertEqual(
                journal_mod.Journal(tmp / "events.jsonl").replay()[-1].event,
                "DELIVERY_OUTCOME_UNKNOWN",
            )

            # Reconciliation: thread provably empty -> RECONCILED, send safe.
            t2 = transport.FakeTransport()
            run_send(tmp, transport=t2, journal=journal)
            self.assertEqual(len(t2.sent_calls), 1)
            self.assertEqual(
                journal_mod.Journal(tmp / "events.jsonl").replay()[-1].event,
                "SENT_CONFIRMED",
            )

    def test_reconcile_converges_to_already_present(self):
        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            build_envelope(tmp)
            journal = journal_mod.Journal(tmp / "events.jsonl")

            t1 = transport.FakeTransport()
            t1.send_failure = RuntimeError("network dropped")
            with self.assertRaises(SystemExit):
                run_send(tmp, transport=t1, journal=journal)

            # The message actually landed: thread now holds the marker.
            body = (tmp / "body.txt").read_text(encoding="utf-8")
            msg, _ = protocol.build_message(body)
            t2 = transport.FakeTransport(user_messages=[msg])
            with self.assertRaises(SystemExit) as ctx:
                run_send(tmp, transport=t2, journal=journal)
            self.assertEqual(ctx.exception.code, 0)
            self.assertEqual(len(t2.sent_calls), 0)
            self.assertEqual(
                journal_mod.Journal(tmp / "events.jsonl").replay()[-1].event,
                "ALREADY_PRESENT",
            )

    def test_reconcile_blocks_on_unavailable(self):
        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            build_envelope(tmp)
            journal = journal_mod.Journal(tmp / "events.jsonl")

            t1 = transport.FakeTransport()
            t1.send_failure = RuntimeError("network dropped")
            with self.assertRaises(SystemExit):
                run_send(tmp, transport=t1, journal=journal)

            t2 = transport.FakeTransport(inspect_state="unavailable")
            with self.assertRaises(SystemExit) as ctx:
                run_send(tmp, transport=t2, journal=journal)
            self.assertEqual(ctx.exception.code, 2)
            self.assertEqual(len(t2.sent_calls), 0)

    def test_reconcile_subcommand_empty_thread(self):
        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            build_envelope(tmp)
            journal = journal_mod.Journal(tmp / "events.jsonl")
            t1 = transport.FakeTransport()
            t1.send_failure = RuntimeError("network dropped")
            with self.assertRaises(SystemExit):
                run_send(tmp, transport=t1, journal=journal)
            t2 = transport.FakeTransport()
            with self.assertRaises(SystemExit) as ctx:
                cli.main(
                    ["reconcile", "--envelope", str(tmp / "env.json")],
                    transport=t2,
                    journal=journal,
                    lock_dir=tmp / "locks",
                )
            self.assertEqual(ctx.exception.code, 0)


class TestInspectionClosedState(unittest.TestCase):
    """R2-B2: unknown inspection states fail closed instead of sending."""

    def test_invalid_inspection_state_fails_closed(self):
        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            build_envelope(tmp)
            journal = journal_mod.Journal(tmp / "events.jsonl")

            class BrokenTransport(transport.FakeTransport):
                def inspect_last_user_message(self):
                    return "GARBAGE"

            t = BrokenTransport()
            with self.assertRaises(SystemExit) as ctx:
                run_send(tmp, transport=t, journal=journal)
            self.assertEqual(ctx.exception.code, 2)
            self.assertEqual(len(t.sent_calls), 0)
            self.assertEqual(
                journal_mod.Journal(tmp / "events.jsonl").replay()[-1].event,
                "INSPECTION_UNAVAILABLE",
            )

    def test_message_requires_text(self):
        with self.assertRaises(ValueError):
            transport.ThreadInspection.message("")


class TestCommentConcurrency(unittest.TestCase):
    """R2-B3: comment list -> create is serialized per chat by ChatLock."""

    def test_post_requires_lock_directory(self):
        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            build_envelope(tmp)
            receipt_path = tmp / "receipt.json"
            receipt_path.write_text(make_receipt().to_json(), encoding="utf-8")
            github = github_adapter.FakeGitHub(live_facts())
            with self.assertRaises(SystemExit):
                cli.main(
                    ["post", "--envelope", str(tmp / "env.json"), "--receipt", str(receipt_path),
                     "--allowed-paths", "docs/A.md"],
                    github=github,
                    journal=journal_mod.Journal(tmp / "events.jsonl"),
                    lock_dir=None,
                )

    def test_concurrent_posts_serialize(self):
        import threading

        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            build_envelope(tmp)
            chat_key, req_sha = env_keys(tmp)
            receipt_path = tmp / "receipt.json"
            receipt_path.write_text(make_receipt().to_json(), encoding="utf-8")
            journal = journal_mod.Journal(tmp / "events.jsonl")
            journal.append(
                event="RECEIPT_PARSED", chat_key=chat_key, request_text_sha256=req_sha
            )
            github = github_adapter.FakeGitHub(live_facts())

            errors: list[BaseException] = []

            def post_once():
                try:
                    cli.main(
                        ["post", "--envelope", str(tmp / "env.json"), "--receipt", str(receipt_path),
                         "--allowed-paths", "docs/A.md"],
                        github=github,
                        journal=journal,
                        lock_dir=tmp / "locks",
                    )
                except SystemExit as exc:
                    if exc.code != 0:
                        errors.append(exc)

            threads = [threading.Thread(target=post_once) for _ in range(4)]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join()
            self.assertEqual(errors, [])
            self.assertEqual(len(github.posted), 1)


class TestMixedCommentMarkers(unittest.TestCase):
    """R2-B4: all markers are scanned before any decision."""

    def test_identical_then_conflict_is_conflict(self):
        receipt = make_receipt()
        receipt_sha = comment_poster.receipt_sha256(receipt)
        other = make_receipt(reviewer="other")
        body_identical = comment_poster.build_comment_body(make_envelope(), receipt)
        body_conflict = comment_poster.build_comment_body(
            models.ReviewRequestEnvelope(
                **{**make_envelope().__dict__, "request_text_sha256": REQ_SHA}
            ),
            other,
        )
        action, _ = comment_poster.reconcile_comments(
            [body_identical, body_conflict], REQ_SHA, receipt_sha
        )
        self.assertEqual(action, "conflict")

    def test_malformed_plus_identical_is_conflict(self):
        receipt = make_receipt()
        receipt_sha = comment_poster.receipt_sha256(receipt)
        body_identical = comment_poster.build_comment_body(make_envelope(), receipt)
        action, _ = comment_poster.reconcile_comments(
            [body_identical, "<!-- independent-review-receipt:broken -->"],
            REQ_SHA,
            receipt_sha,
        )
        self.assertEqual(action, "conflict")


class TestJournalConcurrency(unittest.TestCase):
    """R2-B5: global journal flock keeps the chain intact across writers."""

    def test_concurrent_appends_keep_chain(self):
        import threading

        with tempfile.TemporaryDirectory() as tmpd:
            path = pathlib.Path(tmpd) / "events.jsonl"
            journal = journal_mod.Journal(path)
            errors: list[BaseException] = []

            def append(event_name: str):
                try:
                    for _ in range(20):
                        journal.append(
                            event=event_name, chat_key=CHAT, request_text_sha256=REQ_SHA
                        )
                except BaseException as exc:
                    errors.append(exc)

            threads = [
                threading.Thread(target=append, args=(name,))
                for name in ("BUILT", "SENT_CONFIRMED", "RESPONSE_CAPTURED")
            ]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join()
            self.assertEqual(errors, [])
            records = journal.replay()
            self.assertEqual(len(records), 60)
            self.assertEqual([r.seq for r in records], list(range(1, 61)))


class TestStateMachineEnforced(unittest.TestCase):
    """R2-B6: the CLI rejects journal transitions the state machine forbids."""

    def test_poll_before_delivery_rejected(self):
        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            journal = journal_mod.Journal(tmp / "events.jsonl")
            t = transport.FakeTransport(assistant_replies=["reply"])
            with self.assertRaises(SystemExit):
                cli.main(
                    ["poll", "--chat-key", CHAT, "--request-text-sha256", REQ_SHA,
                     "--out", str(tmp / "reply.json")],
                    transport=t,
                    journal=journal,
                )

    def test_poll_no_reply_is_response_unavailable(self):
        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            build_envelope(tmp)
            chat_key, req_sha = env_keys(tmp)
            journal = journal_mod.Journal(tmp / "events.jsonl")
            journal.append(event="SENT_CONFIRMED", chat_key=chat_key, request_text_sha256=req_sha)
            t = transport.FakeTransport(assistant_replies=[""])
            with self.assertRaises(SystemExit) as ctx:
                cli.main(
                    ["poll", "--chat-key", chat_key, "--request-text-sha256", req_sha,
                     "--out", str(tmp / "reply.json")],
                    transport=t,
                    journal=journal,
                )
            self.assertEqual(ctx.exception.code, 2)
            self.assertEqual(
                journal.replay()[-1].event, "RESPONSE_UNAVAILABLE"
            )

    def test_parse_failure_is_receipt_rejected(self):
        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            build_envelope(tmp)
            chat_key, req_sha = env_keys(tmp)
            journal = journal_mod.Journal(tmp / "events.jsonl")
            journal.append(
                event="RESPONSE_CAPTURED", chat_key=chat_key, request_text_sha256=req_sha
            )
            reply_path = tmp / "reply.md"
            reply_path.write_text("no json here", encoding="utf-8")
            with self.assertRaises(SystemExit) as ctx:
                cli.main(
                    ["parse", "--envelope", str(tmp / "env.json"), "--reply", str(reply_path),
                     "--out", str(tmp / "receipt.json")],
                    journal=journal,
                )
            self.assertEqual(ctx.exception.code, 1)
            self.assertEqual(journal.replay()[-1].event, "RECEIPT_REJECTED")


class TestEvidenceBinding(unittest.TestCase):
    """R2-B7: evidence index path/hash pair is mandatory and revalidated."""

    def test_build_requires_evidence_pair(self):
        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            request_path = tmp / "request.md"
            request_path.write_text("request", encoding="utf-8")
            with self.assertRaises(SystemExit):
                cli.main(
                    ["build", "--request", str(request_path), "--repository", REPO,
                     "--pr", "349", "--base-sha", BASE, "--head-sha", HEAD,
                     "--chat-key", CHAT, "--evidence-index", "",
                     "--evidence-index-sha256", "x" * 64, "--out", str(tmp / "e.json"),
                     "--message-out", str(tmp / "m.txt"), "--body-out", str(tmp / "b.txt")],
                )

    def test_send_revalidates_evidence(self):
        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            build_envelope(tmp)
            journal = journal_mod.Journal(tmp / "events.jsonl")
            # Tamper with the evidence file after build.
            index = json.loads((tmp / "index.json").read_text(encoding="utf-8"))
            index["files"][0]["sha256"] = "f" * 64
            (tmp / "index.json").write_text(json.dumps(index), encoding="utf-8")
            t = transport.FakeTransport()
            with self.assertRaises(SystemExit) as ctx:
                run_send(tmp, transport=t, journal=journal)
            self.assertEqual(ctx.exception.code, 1)
            self.assertEqual(len(t.sent_calls), 0)


class TestLiveIdentity(unittest.TestCase):
    """R2-B8: observed repository/PR identity is validated, not caller-asserted."""

    def test_observed_repository_mismatch_rejected(self):
        errors = live_validation.validate_pr_live_state(
            repository=REPO,
            pr_number=349,
            observed_repository="Other/Repo",
            observed_pr_number=349,
            observed_state="OPEN",
            observed_is_draft=True,
            observed_base_sha=BASE,
            observed_head_sha=HEAD,
            expected_base_sha=BASE,
            expected_head_sha=HEAD,
            observed_merged=False,
        )
        self.assertTrue(any("repository mismatch" in e for e in errors))

    def test_observed_pr_mismatch_rejected(self):
        errors = live_validation.validate_pr_live_state(
            repository=REPO,
            pr_number=349,
            observed_repository=REPO,
            observed_pr_number=999,
            observed_state="OPEN",
            observed_is_draft=True,
            observed_base_sha=BASE,
            observed_head_sha=HEAD,
            expected_base_sha=BASE,
            expected_head_sha=HEAD,
            observed_merged=False,
        )
        self.assertTrue(any("PR mismatch" in e for e in errors))

    def test_send_rejects_fetch_misrouted_to_other_pr(self):
        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            build_envelope(tmp)
            journal = journal_mod.Journal(tmp / "events.jsonl")
            t = transport.FakeTransport()
            github = github_adapter.FakeGitHub({**live_facts(), "pr_number": 999})
            with self.assertRaises(SystemExit) as ctx:
                run_send(tmp, transport=t, github=github, journal=journal)
            self.assertEqual(ctx.exception.code, 1)
            self.assertEqual(len(t.sent_calls), 0)


class TestCliOrchestration(unittest.TestCase):
    """R2-B9: the committed suite exercises the real CLI effect paths."""

    def test_full_chain_build_send_poll_parse_post(self):
        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            env_path = build_envelope(tmp)
            chat_key, req_sha = env_keys(tmp)
            journal = journal_mod.Journal(tmp / "events.jsonl")
            t = transport.FakeTransport()
            g = github_adapter.FakeGitHub(live_facts())

            run_send(tmp, transport=t, github=g, journal=journal)
            self.assertEqual(len(t.sent_calls), 1)
            self.assertEqual(journal.replay()[-1].event, "SENT_CONFIRMED")

            receipt = make_receipt()
            reply_text = "prose\n```json\n" + receipt.to_json() + "\n```\n"
            t2 = transport.FakeTransport(assistant_replies=[reply_text])
            cli.main(
                ["poll", "--chat-key", chat_key, "--request-text-sha256", req_sha,
                 "--out", str(tmp / "reply.md")],
                transport=t2,
                journal=journal,
            )
            self.assertEqual(journal.replay()[-1].event, "RESPONSE_CAPTURED")

            cli.main(
                ["parse", "--envelope", str(env_path), "--reply", str(tmp / "reply.md"),
                 "--out", str(tmp / "receipt.json")],
                journal=journal,
            )
            self.assertEqual(journal.replay()[-1].event, "RECEIPT_PARSED")

            cli.main(
                ["post", "--envelope", str(env_path), "--receipt", str(tmp / "receipt.json"),
                 "--allowed-paths", "docs/A.md"],
                github=g,
                journal=journal,
                lock_dir=tmp / "locks",
            )
            self.assertEqual(len(g.posted), 1)
            self.assertEqual(journal.replay()[-1].event, "COMMENT_POSTED")

            # Idempotent re-post is a skip with zero new comments (exit 0).
            with self.assertRaises(SystemExit) as ctx:
                cli.main(
                    ["post", "--envelope", str(env_path), "--receipt", str(tmp / "receipt.json"),
                     "--allowed-paths", "docs/A.md"],
                    github=g,
                    journal=journal,
                    lock_dir=tmp / "locks",
                )
            self.assertEqual(ctx.exception.code, 0)
            self.assertEqual(len(g.posted), 1)

    def test_post_outcome_unknown_then_reconcile(self):
        from review_loop import cli

        with tempfile.TemporaryDirectory() as tmpd:
            tmp = pathlib.Path(tmpd)
            env_path = build_envelope(tmp)
            chat_key, req_sha = env_keys(tmp)
            journal = journal_mod.Journal(tmp / "events.jsonl")
            journal.append(
                event="RECEIPT_PARSED", chat_key=chat_key, request_text_sha256=req_sha
            )
            receipt_path = tmp / "receipt.json"
            receipt_path.write_text(make_receipt().to_json(), encoding="utf-8")
            g = github_adapter.FakeGitHub(live_facts())
            g.fail_next_post = True
            with self.assertRaises(SystemExit) as ctx:
                cli.main(
                    ["post", "--envelope", str(env_path), "--receipt", str(receipt_path),
                     "--allowed-paths", "docs/A.md"],
                    github=g,
                    journal=journal,
                    lock_dir=tmp / "locks",
                )
            self.assertEqual(ctx.exception.code, 2)
            self.assertEqual(journal.replay()[-1].event, "DELIVERY_OUTCOME_UNKNOWN")
            # Retry after reconciliation (the comment never landed).
            cli.main(
                ["post", "--envelope", str(env_path), "--receipt", str(receipt_path),
                 "--allowed-paths", "docs/A.md"],
                github=g,
                journal=journal,
                lock_dir=tmp / "locks",
            )
            self.assertEqual(len(g.posted), 1)


if __name__ == "__main__":
    unittest.main()
