"""Delivery state machine for the review-loop transport (pure logic).

The critical invariant: an outcome-unknown delivery is never retried blindly.
A caller must reconcile the thread (or abort) before any resend; this module
simply refuses to authorize a send when the outcome is unknown.

Every journal append in the CLI must pass through ``transition_allowed`` so
the persisted sequence can never drift from the state machine (R2-B6).
"""

from __future__ import annotations

from . import models

ORDER = [
    models.DeliveryOutcome.BUILT,
    models.DeliveryOutcome.LIVE_VALIDATED,
    models.DeliveryOutcome.DELIVERY_INSPECTED,
    models.DeliveryOutcome.SENT_CONFIRMED,
    models.DeliveryOutcome.ALREADY_PRESENT,
    models.DeliveryOutcome.SEND_OUTCOME_UNKNOWN,
    models.DeliveryOutcome.COMMENT_OUTCOME_UNKNOWN,
    models.DeliveryOutcome.RECONCILED,
    models.DeliveryOutcome.RESPONSE_CAPTURED,
    models.DeliveryOutcome.RESPONSE_UNAVAILABLE,
    models.DeliveryOutcome.RECEIPT_PARSED,
    models.DeliveryOutcome.RECEIPT_REJECTED,
    models.DeliveryOutcome.HEAD_REVALIDATED,
    models.DeliveryOutcome.COMMENT_POSTED,
    models.DeliveryOutcome.COMPLETE,
]

# Effect-blocking outcomes: no blind retry until the thread/comments are
# reconciled.  These are NOT durable dead-ends; read-only reconciliation is
# the only authorized way out (send marker check, comment re-query).
BLOCKED_UNTIL_RECONCILED = {
    models.DeliveryOutcome.SEND_OUTCOME_UNKNOWN,
    models.DeliveryOutcome.COMMENT_OUTCOME_UNKNOWN,
    models.DeliveryOutcome.FAILED,
}

# Durable dead-end: no authorized transition at all.
TERMINAL = {
    models.DeliveryOutcome.COMPLETE,
    models.DeliveryOutcome.FAILED,
}

# Send-side effect-unknown outcomes: reconciliation is the thread marker
# check only.  A comment-side unknown must never be consumed by the send path.
SEND_BLOCKED = {
    models.DeliveryOutcome.SEND_OUTCOME_UNKNOWN,
}

# Comment-side effect-unknown outcomes: reconciliation is the comment re-query
# only.  A send-side unknown must never be consumed by the post path.
COMMENT_BLOCKED = {
    models.DeliveryOutcome.COMMENT_OUTCOME_UNKNOWN,
}

# Pre-effect failures (auth expired, inspection unavailable) are recoverable:
# retrying the same request after the condition clears is safe because the
# send effect provably never happened.  These are distinct from the
# outcome-unknown states (the effect itself is uncertain).
RECOVERABLE_PRE_EFFECT = {
    models.DeliveryOutcome.AUTH_REQUIRED,
    models.DeliveryOutcome.INSPECTION_UNAVAILABLE,
}

TERMINAL_MESSAGE_OUTCOMES = {
    models.DeliveryOutcome.ALREADY_PRESENT,
    models.DeliveryOutcome.SENT_CONFIRMED,
    models.DeliveryOutcome.SEND_OUTCOME_UNKNOWN,
    models.DeliveryOutcome.COMMENT_OUTCOME_UNKNOWN,
}

# Explicit transition table: current -> allowed observed outcomes.
# The CLI records nothing at build time; a fresh send starts from None with a
# LIVE_VALIDATED append, so None must allow it (plus pre-effect failures).
#
# R3-B1/R3-B2: DELIVERY_INSPECTED (send in-flight) and HEAD_REVALIDATED
# (comment in-flight) are restart-recoverable: after a hard interruption the
# journal may be stuck at the in-flight state, and reconciliation is the only
# authorized next step.  The recovery edges are deliberately limited to the
# convergence outcomes plus the same-phase retry.
_EDGES: dict[models.DeliveryOutcome | None, set[models.DeliveryOutcome]] = {
    None: {
        models.DeliveryOutcome.LIVE_VALIDATED,
        models.DeliveryOutcome.BUILT,
        models.DeliveryOutcome.FAILED,
        models.DeliveryOutcome.AUTH_REQUIRED,
        models.DeliveryOutcome.INSPECTION_UNAVAILABLE,
    },
    models.DeliveryOutcome.BUILT: {
        models.DeliveryOutcome.LIVE_VALIDATED,
        models.DeliveryOutcome.FAILED,
    },
    models.DeliveryOutcome.LIVE_VALIDATED: {
        models.DeliveryOutcome.DELIVERY_INSPECTED,
        models.DeliveryOutcome.AUTH_REQUIRED,
        models.DeliveryOutcome.INSPECTION_UNAVAILABLE,
        models.DeliveryOutcome.FAILED,
    },
    # AUTH_REQUIRED / INSPECTION_UNAVAILABLE: retry re-validates live state.
    models.DeliveryOutcome.AUTH_REQUIRED: {
        models.DeliveryOutcome.LIVE_VALIDATED,
        models.DeliveryOutcome.FAILED,
    },
    models.DeliveryOutcome.INSPECTION_UNAVAILABLE: {
        models.DeliveryOutcome.LIVE_VALIDATED,
        models.DeliveryOutcome.FAILED,
    },
    # Send in-flight: the effect may or may not have landed.  Restart must
    # reconcile the thread: identical marker -> ALREADY_PRESENT, provably
    # empty -> RECONCILED (resend safe), anything else stays blocked.
    models.DeliveryOutcome.DELIVERY_INSPECTED: {
        models.DeliveryOutcome.SENT_CONFIRMED,
        models.DeliveryOutcome.ALREADY_PRESENT,
        models.DeliveryOutcome.SEND_OUTCOME_UNKNOWN,
        models.DeliveryOutcome.RECONCILED,
        models.DeliveryOutcome.FAILED,
    },
    models.DeliveryOutcome.SENT_CONFIRMED: {
        models.DeliveryOutcome.RESPONSE_CAPTURED,
        models.DeliveryOutcome.RESPONSE_UNAVAILABLE,
    },
    models.DeliveryOutcome.ALREADY_PRESENT: {
        models.DeliveryOutcome.RESPONSE_CAPTURED,
        models.DeliveryOutcome.RESPONSE_UNAVAILABLE,
    },
    # Send-side outcome-unknown: only read-only thread reconciliation may
    # follow.  The marker check can converge to ALREADY_PRESENT (already
    # delivered) or to RECONCILED (thread provably empty, resend safe).
    models.DeliveryOutcome.SEND_OUTCOME_UNKNOWN: {
        models.DeliveryOutcome.ALREADY_PRESENT,
        models.DeliveryOutcome.RECONCILED,
        models.DeliveryOutcome.FAILED,
    },
    # Comment-side outcome-unknown: only a comment re-query may follow.
    # Converges to COMMENT_POSTED (comment landed) or back to
    # HEAD_REVALIDATED (provably absent, re-post authorized).
    models.DeliveryOutcome.COMMENT_OUTCOME_UNKNOWN: {
        models.DeliveryOutcome.COMMENT_POSTED,
        models.DeliveryOutcome.HEAD_REVALIDATED,
        models.DeliveryOutcome.FAILED,
    },
    models.DeliveryOutcome.RECONCILED: {
        models.DeliveryOutcome.LIVE_VALIDATED,
        models.DeliveryOutcome.FAILED,
    },
    models.DeliveryOutcome.RESPONSE_CAPTURED: {
        models.DeliveryOutcome.RECEIPT_PARSED,
        models.DeliveryOutcome.RECEIPT_REJECTED,
        models.DeliveryOutcome.FAILED,
    },
    models.DeliveryOutcome.RESPONSE_UNAVAILABLE: {
        models.DeliveryOutcome.RESPONSE_CAPTURED,
        models.DeliveryOutcome.FAILED,
    },
    models.DeliveryOutcome.RECEIPT_PARSED: {
        models.DeliveryOutcome.HEAD_REVALIDATED,
        models.DeliveryOutcome.FAILED,
    },
    models.DeliveryOutcome.RECEIPT_REJECTED: {
        models.DeliveryOutcome.RECEIPT_PARSED,
        models.DeliveryOutcome.FAILED,
    },
    # Comment in-flight: the comment may or may not have landed.  Restart
    # must re-query comments: present identical receipt -> COMMENT_POSTED,
    # provably absent -> revalidate (fresh live facts) then post again.
    models.DeliveryOutcome.HEAD_REVALIDATED: {
        models.DeliveryOutcome.HEAD_REVALIDATED,
        models.DeliveryOutcome.COMMENT_POSTED,
        models.DeliveryOutcome.COMMENT_OUTCOME_UNKNOWN,
        models.DeliveryOutcome.FAILED,
    },
    models.DeliveryOutcome.COMMENT_POSTED: {
        models.DeliveryOutcome.COMPLETE,
        models.DeliveryOutcome.FAILED,
    },
    models.DeliveryOutcome.COMPLETE: set(),
    models.DeliveryOutcome.FAILED: set(),
}


def transition_allowed(
    current: models.DeliveryOutcome | None,
    observed: models.DeliveryOutcome,
) -> bool:
    """Whether the observed outcome is an authorized transition from current."""
    allowed = _EDGES.get(current)
    if allowed is None:
        return False
    return observed in allowed


def next_state(
    current: models.DeliveryOutcome | None,
    observed: models.DeliveryOutcome,
) -> models.DeliveryOutcome | None:
    """Validate a transition; return the resulting state or None if invalid."""
    if transition_allowed(current, observed):
        return observed
    return None


def can_send(current: models.DeliveryOutcome | None) -> bool:
    """A send is authorized only from a provable non-delivered state."""
    if current is None:
        return True
    return current in {models.DeliveryOutcome.BUILT, models.DeliveryOutcome.LIVE_VALIDATED}


def can_poll(current: models.DeliveryOutcome | None) -> bool:
    if current is None:
        return False
    return current in {
        models.DeliveryOutcome.SENT_CONFIRMED,
        models.DeliveryOutcome.ALREADY_PRESENT,
        models.DeliveryOutcome.RESPONSE_CAPTURED,
        models.DeliveryOutcome.RESPONSE_UNAVAILABLE,
    }
