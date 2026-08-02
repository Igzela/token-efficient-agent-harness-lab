"""CLI orchestration for the review-loop transport (repository-owned logic).

Subcommands: build, send, reconcile, poll, parse, post, status.  The browser
transport and the read-only GitHub adapter are injected by the operator; CI
exercises everything with fakes.  All decisions (resend, PASS meaning, comment
posting, live-state acceptance) come from the pure modules and the read-only
adapter, never from caller-asserted JSON or from the transport.

Every journal append passes through the state machine (R2-B6); an invalid
transition fails closed instead of corrupting the chain.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import uuid
from typing import Any, Callable

from . import (
    comment_poster,
    github_adapter,
    journal as journal_mod,
    live_validation,
    locking,
    models,
    protocol,
    receipt_parser,
    state_machine,
)
from .transport import ThreadInspection, Transport, VALID_INSPECTION_STATES


def _fail(message: str, code: int = 1) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(code)


def _record(
    journal: journal_mod.Journal,
    current: models.DeliveryOutcome | None,
    chat_key: str,
    request_sha: str,
    observed: models.DeliveryOutcome,
    detail: str,
) -> models.DeliveryOutcome:
    """Append one journal event only when the state machine allows it (R2-B6).

    The transition is validated atomically inside the journal's global flock
    against the ACTUAL latest event for this (chat, request) (R4-B2), so a
    stale caller-side state can never produce a forbidden sequence.
    """
    try:
        journal.transition_append(
            event=observed.value,
            chat_key=chat_key,
            request_text_sha256=request_sha,
            detail=detail,
        )
    except journal_mod.TransitionRejected as exc:
        _fail(
            f"invalid journal transition {exc.current} -> {exc.observed}; "
            "state machine rejected the append",
            2,
        )
    return observed


def cmd_build(args: argparse.Namespace) -> None:
    request_path = pathlib.Path(args.request)
    if not request_path.exists():
        _fail(f"request file not found: {request_path}")
    request_text = request_path.read_text(encoding="utf-8")
    if not request_text.strip():
        _fail("empty request text")
    request_sha = protocol.request_sha256(request_text)
    # R2-B7: evidence index path and hash are a mandatory pair; the claimed
    # hash is verified against the file now, and re-verified before send.
    if not args.evidence_index:
        _fail("--evidence-index path is required (path/hash pair, R2-B7)")
    if not args.evidence_index_sha256:
        _fail("--evidence-index-sha256 is required (path/hash pair, R2-B7)")
    errors, _ = live_validation.validate_evidence_index(
        pathlib.Path(args.evidence_index), args.evidence_index_sha256
    )
    if errors:
        _fail("evidence index validation failed: " + "; ".join(errors[:10]))
    envelope = models.ReviewRequestEnvelope(
        schema_version=models.ENVELOPE_SCHEMA,
        repository=args.repository,
        pr_number=args.pr,
        base_sha=args.base_sha,
        head_sha=args.head_sha,
        chat_key=args.chat_key,
        evidence_index_sha256=args.evidence_index_sha256,
        request_text_sha256=request_sha,
        implementation_session_id=args.session_id or uuid.uuid4().hex,
    )
    errors = envelope.validate()
    if errors:
        _fail("; ".join(errors))
    out = pathlib.Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(envelope.to_json(), encoding="utf-8")
    message, _ = protocol.build_message(request_text)
    body = protocol.canonical_body(request_text)
    message_path = pathlib.Path(args.message_out)
    message_path.parent.mkdir(parents=True, exist_ok=True)
    message_path.write_text(message, encoding="utf-8")
    body_path = pathlib.Path(args.body_out)
    body_path.parent.mkdir(parents=True, exist_ok=True)
    body_path.write_text(body, encoding="utf-8")
    print(envelope.to_json())
    print(f"delivery marker: {protocol.marker_line(request_sha)}")
    print(f"message {len(message)} bytes written to {message_path}")
    print(f"canonical body {len(body)} bytes written to {body_path}")
    print(f"request_text_sha256: {request_sha}")


def _load_envelope(args: argparse.Namespace) -> models.ReviewRequestEnvelope:
    path = pathlib.Path(args.envelope)
    if not path.exists():
        _fail(f"envelope not found: {path}")
    envelope = models.ReviewRequestEnvelope.from_json(path.read_text(encoding="utf-8"))
    errors = envelope.validate()
    if errors:
        _fail("; ".join(errors))
    return envelope


def _live_facts(
    github: Any,
    envelope: models.ReviewRequestEnvelope,
    allowed_paths: tuple[str, ...],
) -> dict[str, Any]:
    """Mandatory live validation via the read-only adapter (B13/B14/B15/R2-B8).

    The adapter is the only source of live facts; no caller JSON is accepted.
    The observed repository/PR identity is checked against the envelope.
    """
    live = github.fetch_pr(envelope.repository, envelope.pr_number)
    if not live:
        _fail("live GitHub fetch returned no PR facts")
    errors = live_validation.validate_pr_live_state(
        repository=envelope.repository,
        pr_number=envelope.pr_number,
        observed_repository=str(live.get("repository", "")),
        observed_pr_number=int(live.get("pr_number", -1)),
        observed_state=str(live.get("state", "")),
        observed_is_draft=bool(live.get("is_draft")),
        observed_base_sha=str(live.get("base_sha", "")),
        observed_head_sha=str(live.get("head_sha", "")),
        expected_base_sha=envelope.base_sha,
        expected_head_sha=envelope.head_sha,
        observed_merged=bool(live.get("merged")),
    )
    if errors:
        _fail("live PR validation failed: " + "; ".join(errors))
    changed = live.get("changed_files") or []
    diff_errors = live_validation.validate_diff_scope(changed, allowed_paths)
    if diff_errors:
        _fail("diff scope validation failed: " + "; ".join(diff_errors))
    return live


def _revalidate_evidence(args: argparse.Namespace, envelope: models.ReviewRequestEnvelope) -> None:
    """R2-B7: re-verify the evidence index right before the send effect."""
    if not args.evidence_index:
        _fail("--evidence-index path is required at send (R2-B7)")
    errors, _ = live_validation.validate_evidence_index(
        pathlib.Path(args.evidence_index), envelope.evidence_index_sha256
    )
    if errors:
        _fail("evidence index revalidation failed: " + "; ".join(errors[:10]))


def _journal_state(
    journal: journal_mod.Journal, chat_key: str, request_sha: str
) -> models.DeliveryOutcome | None:
    """Rebuild the latest journal event for (chat, request) (B4)."""
    latest: models.DeliveryOutcome | None = None
    for record in journal.replay():
        if record.chat_key == chat_key and record.request_text_sha256 == request_sha:
            try:
                latest = models.DeliveryOutcome(record.event)
            except ValueError:
                continue
    return latest


def _inspect(transport: Transport) -> ThreadInspection | None:
    """Inspect the thread with a closed-state guard (R2-B2).

    Returns None when the transport returned something that is not a valid
    ThreadInspection (a transport bug); the caller decides how to record it.
    """
    inspection = transport.inspect_last_user_message()
    if not isinstance(inspection, ThreadInspection) or inspection.state not in VALID_INSPECTION_STATES:
        return None
    return inspection


def _record_inspection_unavailable(
    journal: journal_mod.Journal,
    current: models.DeliveryOutcome | None,
    envelope: models.ReviewRequestEnvelope,
    request_text_sha: str,
    detail: str,
) -> models.DeliveryOutcome:
    """Record the recoverable pre-effect inspection failure (R2-B1/R2-B2)."""
    return _record(
        journal,
        current,
        envelope.chat_key,
        request_text_sha,
        models.DeliveryOutcome.INSPECTION_UNAVAILABLE,
        detail,
    )


def _reconcile(
    transport: Transport,
    journal: journal_mod.Journal,
    envelope: models.ReviewRequestEnvelope,
    request_text_sha: str,
    current: models.DeliveryOutcome | None,
) -> tuple[models.DeliveryOutcome, str]:
    """Read-only thread reconciliation (R2-B1/R3-B1).

    Authorized from SEND_OUTCOME_UNKNOWN (effect uncertain) and from
    DELIVERY_INSPECTED (send in-flight after a hard interruption).

    Returns (latest_state, action) where action is one of:
    - "already_present": the identical marker is in the thread (converged).
    - "may_send": the thread is provably empty; a resend is authorized.
    - "blocked": reconciliation cannot prove non-delivery; keep blocking.
    """
    inspection = _inspect(transport)
    if inspection is None or inspection.state == "INSPECTION_UNAVAILABLE":
        # Do not overwrite the blocked state; keep blocking without a new
        # event (INSPECTION_UNAVAILABLE is not reachable from these states).
        return current, "blocked"
    if inspection.state == "MESSAGE":
        existing_sha = protocol.extract_marker(inspection.text)
        if existing_sha == request_text_sha:
            return (
                _record(
                    journal,
                    current,
                    envelope.chat_key,
                    request_text_sha,
                    models.DeliveryOutcome.ALREADY_PRESENT,
                    f"reconcile: identical marker {existing_sha[:12]}... in thread",
                ),
                "already_present",
            )
        _fail(
            f"SEND_OUTCOME_UNKNOWN: thread holds a different message; "
            "reconciliation cannot prove non-delivery; manual review required",
            2,
        )
    if inspection.state == "EMPTY_THREAD":
        return (
            _record(
                journal,
                current,
                envelope.chat_key,
                request_text_sha,
                models.DeliveryOutcome.RECONCILED,
                "reconcile: thread provably empty; resend is safe",
            ),
            "may_send",
        )
    return current, "blocked"


def cmd_send(
    args: argparse.Namespace,
    transport: Transport,
    journal: journal_mod.Journal,
    github: Any,
) -> None:
    envelope = _load_envelope(args)
    body_path = pathlib.Path(args.body)
    if not body_path.exists():
        _fail(f"canonical body not found: {body_path}; run build first")
    body = body_path.read_text(encoding="utf-8")
    request_text_sha = protocol.request_sha256(body)
    if request_text_sha != envelope.request_text_sha256:
        _fail("canonical body does not hash to the envelope sha")

    # B4/R2-B1/R3-B1: rebuild persisted state.  Both the send-side unknown and
    # the send in-flight state (DELIVERY_INSPECTED, stuck after a hard
    # interruption between the effect and the journal write) require read-only
    # thread reconciliation before any resend.
    current = _journal_state(journal, envelope.chat_key, request_text_sha)
    if current in {models.DeliveryOutcome.SENT_CONFIRMED, models.DeliveryOutcome.ALREADY_PRESENT}:
        # Idempotent re-run of an already-confirmed delivery.
        print(f"ALREADY_PRESENT: sha {request_text_sha[:12]}... already delivered "
              f"(latest {current.value})")
        raise SystemExit(0)
    if current in state_machine.SEND_BLOCKED or current == models.DeliveryOutcome.DELIVERY_INSPECTED:
        print(
            f"{current.value} recorded; running read-only "
            "reconciliation before any resend"
        )
        current, action = _reconcile(
            transport, journal, envelope, request_text_sha, current
        )
        if action == "already_present":
            print(f"ALREADY_PRESENT: sha {request_text_sha[:12]}... already delivered")
            raise SystemExit(0)
        if action == "blocked":
            print("SEND_OUTCOME_UNKNOWN: still unresolved; reconcile the thread manually")
            raise SystemExit(2)
        print("reconciled: thread provably empty; resend authorized")

    # R2-B7: re-verify the evidence index immediately before the send effect.
    _revalidate_evidence(args, envelope)

    live = _live_facts(github, envelope, tuple(args.allowed_paths or []))
    current = _record(
        journal,
        current,
        envelope.chat_key,
        request_text_sha,
        models.DeliveryOutcome.LIVE_VALIDATED,
        "live PR and diff validated via read-only adapter",
    )

    if not transport.read_auth_state():
        # Pre-effect failure: recoverable, NOT effect-unknown (R2-B1).
        current = _record(
            journal,
            current,
            envelope.chat_key,
            request_text_sha,
            models.DeliveryOutcome.AUTH_REQUIRED,
            "login expired; no send attempted; retry after login",
        )
        print("AUTH_REQUIRED: login expired; no send attempted")
        raise SystemExit(2)

    inspection = _inspect(transport)
    if inspection is None:
        # R2-B2: a transport bug must fail closed, never fall through to send.
        current = _record_inspection_unavailable(
            journal,
            current,
            envelope,
            request_text_sha,
            f"transport returned invalid inspection state; fail closed",
        )
        print("INSPECTION_UNAVAILABLE: invalid inspection state from transport; no send attempted")
        raise SystemExit(2)
    if inspection.state == "INSPECTION_UNAVAILABLE":
        current = _record_inspection_unavailable(
            journal,
            current,
            envelope,
            request_text_sha,
            "thread inspection unavailable; pre-effect; retry is safe",
        )
        print("INSPECTION_UNAVAILABLE: cannot prove thread state; no send attempted")
        raise SystemExit(2)

    existing_sha = None
    if inspection.state == "MESSAGE":
        existing_sha = protocol.extract_marker(inspection.text)
        if existing_sha == request_text_sha:
            current = _record(
                journal,
                current,
                envelope.chat_key,
                request_text_sha,
                models.DeliveryOutcome.ALREADY_PRESENT,
                f"identical marker {existing_sha[:12]}... already in thread",
            )
            print(f"ALREADY_PRESENT: sha {request_text_sha[:12]}... already delivered")
            raise SystemExit(0)
    current = _record(
        journal,
        current,
        envelope.chat_key,
        request_text_sha,
        models.DeliveryOutcome.DELIVERY_INSPECTED,
        f"inspection={inspection.state} prior_marker={existing_sha or 'NONE'}; sending",
    )

    message, _ = protocol.build_message(body)
    try:
        transport.send_user_message(message)
    except Exception as exc:
        # B4/R2-B1/R3-B3: a raised send means the send effect is unknown.
        # SEND_OUTCOME_UNKNOWN is send-phase specific; only thread
        # reconciliation may unblock it (never the comment path).
        current = _record(
            journal,
            current,
            envelope.chat_key,
            request_text_sha,
            models.DeliveryOutcome.SEND_OUTCOME_UNKNOWN,
            f"send raised {type(exc).__name__}: {str(exc)[:200]}",
        )
        print("SEND_OUTCOME_UNKNOWN: send raised; effect unknown; reconcile before resend")
        raise SystemExit(2)
    current = _record(
        journal,
        current,
        envelope.chat_key,
        request_text_sha,
        models.DeliveryOutcome.SENT_CONFIRMED,
        "message delivered and transport confirmed",
    )
    print(f"SENT_CONFIRMED: sha {request_text_sha[:12]}...")


def cmd_reconcile(
    args: argparse.Namespace,
    transport: Transport,
    journal: journal_mod.Journal,
) -> None:
    """Read-only reconciliation subcommand (R2-B1/R3-B1)."""
    envelope = _load_envelope(args)
    request_text_sha = envelope.request_text_sha256
    current = _journal_state(journal, envelope.chat_key, request_text_sha)
    if current not in state_machine.SEND_BLOCKED and current != models.DeliveryOutcome.DELIVERY_INSPECTED:
        print(f"reconcile: nothing to reconcile (latest {current}); nothing to do")
        raise SystemExit(0)
    current, action = _reconcile(transport, journal, envelope, request_text_sha, current)
    if action == "already_present":
        print("RECONCILED: identical marker found; already delivered")
        raise SystemExit(0)
    if action == "may_send":
        print("RECONCILED: thread provably empty; send is authorized")
        raise SystemExit(0)
    print("SEND_OUTCOME_UNKNOWN: reconciliation could not prove non-delivery; stop")
    raise SystemExit(2)


def cmd_poll(
    args: argparse.Namespace,
    transport: Transport,
    journal: journal_mod.Journal,
) -> None:
    current = _journal_state(journal, args.chat_key, args.request_text_sha256)
    if current is None or not state_machine.can_poll(current):
        _fail(
            f"poll not allowed from journal state {current}; "
            "expected a confirmed delivery first",
            2,
        )
    reply = transport.read_latest_assistant_message()
    if not reply or not reply.strip():
        current = _record(
            journal,
            current,
            args.chat_key,
            args.request_text_sha256,
            models.DeliveryOutcome.RESPONSE_UNAVAILABLE,
            "no assistant reply captured",
        )
        _fail("no assistant reply captured", 2)
    out = pathlib.Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(reply, encoding="utf-8")
    current = _record(
        journal,
        current,
        args.chat_key,
        args.request_text_sha256,
        models.DeliveryOutcome.RESPONSE_CAPTURED,
        f"reply {len(reply)} chars saved to {out}",
    )
    print(f"RESPONSE_CAPTURED: {len(reply)} chars -> {out}")


def cmd_parse(args: argparse.Namespace, journal: journal_mod.Journal) -> None:
    envelope = _load_envelope(args)
    current = _journal_state(journal, envelope.chat_key, envelope.request_text_sha256)
    reply_path = pathlib.Path(args.reply)
    if not reply_path.exists():
        _fail(f"reply not found: {reply_path}")
    receipt, errors = receipt_parser.parse_receipt(reply_path.read_text(encoding="utf-8"))
    if receipt is None:
        current = _record(
            journal,
            current,
            envelope.chat_key,
            envelope.request_text_sha256,
            models.DeliveryOutcome.RECEIPT_REJECTED,
            "parse rejected: " + "; ".join(errors),
        )
        _fail("receipt parse failed: " + "; ".join(errors))
    match_errors = receipt.matches_envelope(envelope)
    if match_errors:
        current = _record(
            journal,
            current,
            envelope.chat_key,
            envelope.request_text_sha256,
            models.DeliveryOutcome.RECEIPT_REJECTED,
            "identity mismatch: " + "; ".join(match_errors),
        )
        _fail("receipt does not match envelope: " + "; ".join(match_errors))
    out = pathlib.Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(receipt.to_json(), encoding="utf-8")
    current = _record(
        journal,
        current,
        envelope.chat_key,
        envelope.request_text_sha256,
        models.DeliveryOutcome.RECEIPT_PARSED,
        f"exact structured PASS parsed -> {out}",
    )
    print(receipt.to_json())
    print(f"RECEIPT_PARSED: exact PASS for head {receipt.head_sha[:12]}...")


def cmd_post(
    args: argparse.Namespace,
    github: Any,
    journal: journal_mod.Journal,
) -> None:
    envelope = _load_envelope(args)
    receipt = models.ReviewReceipt.from_json(
        pathlib.Path(args.receipt).read_text(encoding="utf-8")
    )
    if receipt.validate():
        _fail("stored receipt does not validate")
    if receipt.matches_envelope(envelope):
        _fail("stored receipt does not match envelope")
    current = _journal_state(journal, envelope.chat_key, envelope.request_text_sha256)

    # Post-side reconciliation (R2-B1/R3-B2/R3-B3).  Only comment-phase states
    # may be consumed here: COMMENT_OUTCOME_UNKNOWN (effect uncertain) and
    # HEAD_REVALIDATED (comment in-flight, stuck after a hard interruption
    # between create_comment and the journal write).  A send-phase state is
    # never touched by the post command.
    if current in state_machine.COMMENT_BLOCKED or current == models.DeliveryOutcome.HEAD_REVALIDATED:
        existing = github.list_comments(envelope.repository, envelope.pr_number)
        action, reasons = comment_poster.reconcile_comments(
            existing,
            envelope.request_text_sha256,
            comment_poster.receipt_sha256(receipt),
        )
        if action == "skip":
            current = _record(
                journal,
                current,
                envelope.chat_key,
                envelope.request_text_sha256,
                models.DeliveryOutcome.COMMENT_POSTED,
                "post reconcile: " + "; ".join(reasons),
            )
            print("COMMENT_SKIPPED: " + "; ".join(reasons))
            raise SystemExit(0)
        if action != "post":
            _fail(
                "comment state unknown after post outcome-unknown: " + "; ".join(reasons),
                2,
            )
        print("post reconciled: receipt provably absent; re-posting is safe")

    # Idempotent re-post: the journal already shows a posted comment.  The
    # comments are re-consulted; only an identical receipt may skip.  Anything
    # else is a state/comment disagreement and fails closed.
    if current == models.DeliveryOutcome.COMMENT_POSTED:
        existing = github.list_comments(envelope.repository, envelope.pr_number)
        action, reasons = comment_poster.reconcile_comments(
            existing,
            envelope.request_text_sha256,
            comment_poster.receipt_sha256(receipt),
        )
        if action == "skip":
            print("COMMENT_SKIPPED: " + "; ".join(reasons))
            raise SystemExit(0)
        _fail(
            "comment state disagrees with journal: " + "; ".join(reasons),
            2,
        )

    # B8/B13/B14/B15/R2-B8: mandatory head revalidation via the adapter.
    live = _live_facts(github, envelope, tuple(args.allowed_paths or []))
    current = _record(
        journal,
        current,
        envelope.chat_key,
        envelope.request_text_sha256,
        models.DeliveryOutcome.HEAD_REVALIDATED,
        f"live head {live.get('head_sha', '')[:12]}... revalidated",
    )

    existing = github.list_comments(envelope.repository, envelope.pr_number)
    action, reasons = comment_poster.reconcile_comments(
        existing,
        envelope.request_text_sha256,
        comment_poster.receipt_sha256(receipt),
    )
    if action == "skip":
        current = _record(
            journal,
            current,
            envelope.chat_key,
            envelope.request_text_sha256,
            models.DeliveryOutcome.COMMENT_POSTED,
            "skip; " + "; ".join(reasons),
        )
        print("COMMENT_SKIPPED: " + "; ".join(reasons))
        raise SystemExit(0)
    if action == "conflict":
        _fail("comment conflict: " + "; ".join(reasons))
    if action == "unknown":
        _fail("comment state unknown: " + "; ".join(reasons))
    body = comment_poster.build_comment_body(envelope, receipt)
    try:
        url = github.create_comment(envelope.repository, envelope.pr_number, body)
    except Exception as exc:
        # B10/R2-B1/R3-B3: a raised POST means the comment effect is unknown.
        # COMMENT_OUTCOME_UNKNOWN is comment-phase specific; only a comment
        # re-query may unblock it (never the send path).
        current = _record(
            journal,
            current,
            envelope.chat_key,
            envelope.request_text_sha256,
            models.DeliveryOutcome.COMMENT_OUTCOME_UNKNOWN,
            f"comment POST raised {type(exc).__name__}: {str(exc)[:200]}",
        )
        print("COMMENT_OUTCOME_UNKNOWN: re-query comments before retrying")
        raise SystemExit(2)
    current = _record(
        journal,
        current,
        envelope.chat_key,
        envelope.request_text_sha256,
        models.DeliveryOutcome.COMMENT_POSTED,
        url,
    )
    print(f"COMMENT_POSTED: {url}")


def cmd_status(args: argparse.Namespace, journal: journal_mod.Journal) -> None:
    print(journal_mod.serialize_projection(journal.projection()))


def main(
    argv: list[str] | None = None,
    *,
    transport: Transport | None = None,
    github: Any | None = None,
    journal: journal_mod.Journal | None = None,
    lock_dir: pathlib.Path | None = None,
) -> None:
    parser = argparse.ArgumentParser(prog="review_loop")
    sub = parser.add_subparsers(dest="command", required=True)

    build = sub.add_parser("build")
    build.add_argument("--request", required=True)
    build.add_argument("--repository", required=True)
    build.add_argument("--pr", type=int, required=True)
    build.add_argument("--base-sha", required=True)
    build.add_argument("--head-sha", required=True)
    build.add_argument("--chat-key", required=True)
    build.add_argument("--evidence-index-sha256", required=True)
    build.add_argument("--evidence-index", required=True, help="path to evidence index json")
    build.add_argument("--session-id", default="")
    build.add_argument("--out", required=True)
    build.add_argument("--message-out", required=True)
    build.add_argument("--body-out", required=True)
    build.set_defaults(func=cmd_build)

    send = sub.add_parser("send")
    send.add_argument("--envelope", required=True)
    send.add_argument("--body", required=True)
    send.add_argument("--evidence-index", required=True, help="path to evidence index json (revalidated)")
    send.add_argument("--allowed-paths", nargs="*", default=[])
    send.set_defaults(func=cmd_send)

    reconcile = sub.add_parser("reconcile")
    reconcile.add_argument("--envelope", required=True)
    reconcile.set_defaults(func=cmd_reconcile)

    poll = sub.add_parser("poll")
    poll.add_argument("--chat-key", required=True)
    poll.add_argument("--request-text-sha256", required=True)
    poll.add_argument("--out", required=True)
    poll.set_defaults(func=cmd_poll)

    parse = sub.add_parser("parse")
    parse.add_argument("--envelope", required=True)
    parse.add_argument("--reply", required=True)
    parse.add_argument("--out", required=True)
    parse.set_defaults(func=cmd_parse)

    post = sub.add_parser("post")
    post.add_argument("--envelope", required=True)
    post.add_argument("--receipt", required=True)
    post.add_argument("--allowed-paths", nargs="*", default=[])
    post.set_defaults(func=cmd_post)

    status = sub.add_parser("status")
    status.set_defaults(func=cmd_status)

    args = parser.parse_args(argv)
    if journal is None:
        journal = journal_mod.Journal(pathlib.Path("review-loop-events.jsonl"))
    func: Callable[..., None] = args.func
    if func is cmd_build:
        func(args)
    elif func is cmd_send:
        if transport is None:
            _fail("send requires a transport (operator adapter or fake)")
        if github is None:
            _fail("send requires a read-only GitHub adapter")
        if lock_dir is None:
            _fail("send requires a lock directory")
        envelope = _load_envelope(args)
        with locking.ChatLock(lock_dir, envelope.chat_key):
            func(args, transport, journal, github)
    elif func is cmd_reconcile:
        if transport is None:
            _fail("reconcile requires a transport")
        if lock_dir is None:
            _fail("reconcile requires a lock directory")
        envelope = _load_envelope(args)
        with locking.ChatLock(lock_dir, envelope.chat_key):
            func(args, transport, journal)
    elif func is cmd_poll:
        if transport is None:
            _fail("poll requires a transport")
        if lock_dir is None:
            _fail("poll requires a lock directory")
        # R4-B2: poll holds the per-chat lock so its state check + append
        # cannot race another command on the same chat.
        with locking.ChatLock(lock_dir, args.chat_key):
            func(args, transport, journal)
    elif func is cmd_post:
        if github is None:
            _fail("post requires a read-only GitHub adapter")
        if lock_dir is None:
            _fail("post requires a lock directory")
        # R2-B3: comment list -> create must be serialized per chat.
        envelope = _load_envelope(args)
        with locking.ChatLock(lock_dir, envelope.chat_key):
            func(args, github, journal)
    elif func is cmd_parse:
        if lock_dir is None:
            _fail("parse requires a lock directory")
        # R4-B2: parse holds the per-chat lock for the same atomicity reason.
        envelope = _load_envelope(args)
        with locking.ChatLock(lock_dir, envelope.chat_key):
            func(args, journal)
    else:
        func(args, journal)


if __name__ == "__main__":
    main()
