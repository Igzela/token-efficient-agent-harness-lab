"""CLI orchestration for the review-loop transport (repository-owned logic).

Subcommands: build, send, poll, parse, post, status.  The browser transport
and the read-only GitHub adapter are injected by the operator; CI exercises
everything with fakes.  All decisions (resend, PASS meaning, comment posting,
live-state acceptance) come from the pure modules and the read-only adapter,
never from caller-asserted JSON or from the transport.
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
from .transport import ThreadInspection, Transport


def _fail(message: str, code: int = 1) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(code)


def cmd_build(args: argparse.Namespace) -> None:
    request_path = pathlib.Path(args.request)
    if not request_path.exists():
        _fail(f"request file not found: {request_path}")
    request_text = request_path.read_text(encoding="utf-8")
    if not request_text.strip():
        _fail("empty request text")
    request_sha = protocol.request_sha256(request_text)
    evidence_index = args.evidence_index_sha256 or "unavailable"
    if args.evidence_index and evidence_index != "unavailable":
        errors, _ = live_validation.validate_evidence_index(
            pathlib.Path(args.evidence_index), evidence_index
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
        evidence_index_sha256=evidence_index,
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
    """Mandatory live validation via the read-only adapter (B13/B14/B15).

    The adapter is the only source of live facts; no caller JSON is accepted.
    """
    live = github.fetch_pr(envelope.repository, envelope.pr_number)
    if not live:
        _fail("live GitHub fetch returned no PR facts")
    errors = live_validation.validate_pr_live_state(
        repository=envelope.repository,
        pr_number=envelope.pr_number,
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

    # B4: rebuild persisted state and refuse a blind resend after an
    # outcome-unknown (or auth-required) terminal event.
    current = _journal_state(journal, envelope.chat_key, request_text_sha)
    if current in state_machine.RESEND_BLOCKED:
        journal.append(
            event=models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN.value,
            chat_key=envelope.chat_key,
            request_text_sha256=request_text_sha,
            detail=f"refusing resend after terminal event {current.value}",
        )
        print(f"DELIVERY_OUTCOME_UNKNOWN: outcome-unknown terminal {current.value} already "
              "recorded; reconcile the thread before any resend")
        raise SystemExit(2)

    live = _live_facts(github, envelope, tuple(args.allowed_paths or []))
    journal.append(
        event=models.DeliveryOutcome.LIVE_VALIDATED.value,
        chat_key=envelope.chat_key,
        request_text_sha256=request_text_sha,
        detail="live PR and diff validated via read-only adapter",
    )

    if not transport.read_auth_state():
        journal.append(
            event=models.DeliveryOutcome.AUTH_REQUIRED.value,
            chat_key=envelope.chat_key,
            request_text_sha256=request_text_sha,
            detail="login expired; no blind resend",
        )
        print("DELIVERY_OUTCOME_UNKNOWN (auth required); no send")
        raise SystemExit(2)

    inspection = transport.inspect_last_user_message()
    if inspection.state == "INSPECTION_UNAVAILABLE":
        journal.append(
            event=models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN.value,
            chat_key=envelope.chat_key,
            request_text_sha256=request_text_sha,
            detail="thread inspection unavailable; refusing to send",
        )
        print("DELIVERY_OUTCOME_UNKNOWN: thread inspection unavailable; stop")
        raise SystemExit(2)

    existing_sha = None
    if inspection.state == "MESSAGE":
        existing_sha = protocol.extract_marker(inspection.text)
        if existing_sha == request_text_sha:
            journal.append(
                event=models.DeliveryOutcome.ALREADY_PRESENT.value,
                chat_key=envelope.chat_key,
                request_text_sha256=request_text_sha,
                detail=f"identical marker {existing_sha[:12]}... already in thread",
            )
            print(f"ALREADY_PRESENT: sha {request_text_sha[:12]}... already delivered")
            raise SystemExit(0)
    journal.append(
        event=models.DeliveryOutcome.DELIVERY_INSPECTED.value,
        chat_key=envelope.chat_key,
        request_text_sha256=request_text_sha,
        detail=(
            f"inspection={inspection.state} prior_marker={existing_sha or 'NONE'}; sending"
        ),
    )

    message, _ = protocol.build_message(body)
    try:
        transport.send_user_message(message)
    except Exception as exc:
        # B4: an exception means the send effect is unknown; record terminal.
        journal.append(
            event=models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN.value,
            chat_key=envelope.chat_key,
            request_text_sha256=request_text_sha,
            detail=f"send raised {type(exc).__name__}: {str(exc)[:200]}",
        )
        print("DELIVERY_OUTCOME_UNKNOWN: send raised; effect unknown; reconcile before resend")
        raise SystemExit(2)
    journal.append(
        event=models.DeliveryOutcome.SENT_CONFIRMED.value,
        chat_key=envelope.chat_key,
        request_text_sha256=request_text_sha,
        detail="message delivered and transport confirmed",
    )
    print(f"SENT_CONFIRMED: sha {request_text_sha[:12]}...")


def cmd_poll(
    args: argparse.Namespace,
    transport: Transport,
    journal: journal_mod.Journal,
) -> None:
    reply = transport.read_latest_assistant_message()
    if not reply or not reply.strip():
        journal.append(
            event=models.DeliveryOutcome.RESPONSE_CAPTURED.value,
            chat_key=args.chat_key,
            request_text_sha256=args.request_text_sha256,
            detail="no assistant reply captured",
        )
        _fail("no assistant reply captured", 2)
    out = pathlib.Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(reply, encoding="utf-8")
    journal.append(
        event=models.DeliveryOutcome.RESPONSE_CAPTURED.value,
        chat_key=args.chat_key,
        request_text_sha256=args.request_text_sha256,
        detail=f"reply {len(reply)} chars saved to {out}",
    )
    print(f"RESPONSE_CAPTURED: {len(reply)} chars -> {out}")


def cmd_parse(args: argparse.Namespace, journal: journal_mod.Journal) -> None:
    envelope = _load_envelope(args)
    reply_path = pathlib.Path(args.reply)
    if not reply_path.exists():
        _fail(f"reply not found: {reply_path}")
    receipt, errors = receipt_parser.parse_receipt(reply_path.read_text(encoding="utf-8"))
    if receipt is None:
        journal.append(
            event=models.DeliveryOutcome.RECEIPT_PARSED.value,
            chat_key=envelope.chat_key,
            request_text_sha256=envelope.request_text_sha256,
            detail="parse failed: " + "; ".join(errors),
        )
        _fail("receipt parse failed: " + "; ".join(errors))
    match_errors = receipt.matches_envelope(envelope)
    if match_errors:
        journal.append(
            event=models.DeliveryOutcome.RECEIPT_PARSED.value,
            chat_key=envelope.chat_key,
            request_text_sha256=envelope.request_text_sha256,
            detail="identity mismatch: " + "; ".join(match_errors),
        )
        _fail("receipt does not match envelope: " + "; ".join(match_errors))
    out = pathlib.Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(receipt.to_json(), encoding="utf-8")
    journal.append(
        event=models.DeliveryOutcome.RECEIPT_PARSED.value,
        chat_key=envelope.chat_key,
        request_text_sha256=envelope.request_text_sha256,
        detail=f"exact structured PASS parsed -> {out}",
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

    # B8/B13/B14/B15: mandatory head revalidation via the adapter before posting.
    live = _live_facts(github, envelope, tuple(args.allowed_paths or []))
    journal.append(
        event=models.DeliveryOutcome.HEAD_REVALIDATED.value,
        chat_key=envelope.chat_key,
        request_text_sha256=envelope.request_text_sha256,
        detail=f"live head {live.get('head_sha', '')[:12]}... revalidated",
    )

    existing = github.list_comments(envelope.repository, envelope.pr_number)
    action, reasons = comment_poster.reconcile_comments(
        existing,
        envelope.request_text_sha256,
        comment_poster.receipt_sha256(receipt),
    )
    if action == "skip":
        journal.append(
            event=models.DeliveryOutcome.COMMENT_POSTED.value,
            chat_key=envelope.chat_key,
            request_text_sha256=envelope.request_text_sha256,
            detail="; ".join(reasons),
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
        # B10: POST outcome unknown must be recorded; reconciliation required.
        journal.append(
            event=models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN.value,
            chat_key=envelope.chat_key,
            request_text_sha256=envelope.request_text_sha256,
            detail=f"comment POST raised {type(exc).__name__}: {str(exc)[:200]}",
        )
        print("COMMENT_OUTCOME_UNKNOWN: re-query comments before retrying")
        raise SystemExit(2)
    journal.append(
        event=models.DeliveryOutcome.COMMENT_POSTED.value,
        chat_key=envelope.chat_key,
        request_text_sha256=envelope.request_text_sha256,
        detail=url,
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
    build.add_argument("--evidence-index-sha256", default="")
    build.add_argument("--evidence-index", default="", help="path to evidence index json (validated when provided)")
    build.add_argument("--session-id", default="")
    build.add_argument("--out", required=True)
    build.add_argument("--message-out", required=True)
    build.add_argument("--body-out", required=True)
    build.set_defaults(func=cmd_build)

    send = sub.add_parser("send")
    send.add_argument("--envelope", required=True)
    send.add_argument("--body", required=True)
    send.add_argument("--allowed-paths", nargs="*", default=[])
    send.set_defaults(func=cmd_send)

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
    elif func is cmd_poll:
        if transport is None:
            _fail("poll requires a transport")
        func(args, transport, journal)
    elif func is cmd_post:
        if github is None:
            _fail("post requires a read-only GitHub adapter")
        func(args, github, journal)
    else:
        func(args, journal)


if __name__ == "__main__":
    main()
