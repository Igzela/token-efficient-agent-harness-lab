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
    models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN,
    models.DeliveryOutcome.RECONCILED,
    models.DeliveryOutcome.RESPONSE_CAPTURED,
    models.DeliveryOutcome.RESPONSE_UNAVAILABLE,
    models.DeliveryOutcome.RECEIPT_PARSED,
    models.DeliveryOutcome.RECEIPT_REJECTED,
    models.DeliveryOutcome.HEAD_REVALIDATED,
    models.DeliveryOutcome.COMMENT_POSTED,
    models.DeliveryOutcome.COMPLETE,
]

# States that terminate the whole delivery lifecycle and may not transition.
TERMINAL = {
    models.DeliveryOutcome.COMPLETE,
    models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN,
    models.DeliveryOutcome.FAILED,
}

# A terminal outcome that forbids any resend until the thread is reconciled.
# SENT_CONFIRMED / ALREADY_PRESENT are NOT in this set: re-running the same
# request after a confirmed delivery must be allowed so the marker check can
# still report ALREADY_PRESENT instead of double posting.
RESEND_BLOCKED = {
    models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN,
}

# Pre-effect failures (auth expired, inspection unavailable) are recoverable:
# retrying the same request after the condition clears is safe because the
# send effect provably never happened.  These are distinct from
# DELIVERY_OUTCOME_UNKNOWN (the send effect itself is uncertain).
RECOVERABLE_PRE_EFFECT = {
    models.DeliveryOutcome.AUTH_REQUIRED,
    models.DeliveryOutcome.INSPECTION_UNAVAILABLE,
}

TERMINAL_MESSAGE_OUTCOMES = {
    models.DeliveryOutcome.ALREADY_PRESENT,
    models.DeliveryOutcome.SENT_CONFIRMED,
    models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN,
}

# Explicit transition table: current -> allowed observed outcomes.
# The CLI records nothing at build time; a fresh send starts from None with a
# LIVE_VALIDATED append, so None must allow it (plus pre-effect failures).
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
    models.DeliveryOutcome.DELIVERY_INSPECTED: {
        models.DeliveryOutcome.SENT_CONFIRMED,
        models.DeliveryOutcome.ALREADY_PRESENT,
        models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN,
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
    # Outcome-unknown: only read-only reconciliation may follow.  The marker
    # check can converge to ALREADY_PRESENT (already delivered) or to
    # RECONCILED (thread provably empty, so a resend is safe).  For the
    # comment-post side, re-querying comments first may authorize a fresh
    # revalidation (HEAD_REVALIDATED) when the comment provably never landed.
    models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN: {
        models.DeliveryOutcome.ALREADY_PRESENT,
        models.DeliveryOutcome.RECONCILED,
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
    models.DeliveryOutcome.HEAD_REVALIDATED: {
        models.DeliveryOutcome.COMMENT_POSTED,
        models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN,
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
