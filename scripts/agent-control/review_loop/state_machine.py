"""Delivery state machine for the review-loop transport (pure logic).

The critical invariant: an outcome-unknown delivery is never retried blindly.
A caller must reconcile the thread (or abort) before any resend; this module
simply refuses to authorize a send when the outcome is unknown.
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
    models.DeliveryOutcome.RESPONSE_CAPTURED,
    models.DeliveryOutcome.RECEIPT_PARSED,
    models.DeliveryOutcome.HEAD_REVALIDATED,
    models.DeliveryOutcome.COMMENT_POSTED,
    models.DeliveryOutcome.COMPLETE,
]

TERMINAL = {
    models.DeliveryOutcome.COMPLETE,
    models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN,
    models.DeliveryOutcome.AUTH_REQUIRED,
    models.DeliveryOutcome.FAILED,
}

# A terminal outcome that forbids any resend until the thread is reconciled.
# SENT_CONFIRMED / ALREADY_PRESENT are NOT in this set: re-running the same
# request after a confirmed delivery must be allowed so the marker check can
# still report ALREADY_PRESENT instead of double posting.
RESEND_BLOCKED = {
    models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN,
    models.DeliveryOutcome.AUTH_REQUIRED,
}

TERMINAL_MESSAGE_OUTCOMES = {
    models.DeliveryOutcome.ALREADY_PRESENT,
    models.DeliveryOutcome.SENT_CONFIRMED,
    models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN,
}


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
    }


def next_state(
    current: models.DeliveryOutcome | None,
    observed: models.DeliveryOutcome,
) -> models.DeliveryOutcome | None:
    """Validate a transition; return the resulting state or None if invalid."""
    if current is None:
        return observed if observed in {models.DeliveryOutcome.BUILT, models.DeliveryOutcome.FAILED, models.DeliveryOutcome.AUTH_REQUIRED} else None
    if current in TERMINAL:
        return None
    if observed == models.DeliveryOutcome.DELIVERY_OUTCOME_UNKNOWN:
        return observed
    if observed == models.DeliveryOutcome.ALREADY_PRESENT:
        return observed if current in {models.DeliveryOutcome.BUILT, models.DeliveryOutcome.LIVE_VALIDATED, models.DeliveryOutcome.DELIVERY_INSPECTED} else None
    try:
        current_index = ORDER.index(current)
    except ValueError:
        return None
    observed_index = ORDER.index(observed)
    if observed_index == current_index + 1:
        return observed
    if observed_index == current_index + 2 and current in {
        models.DeliveryOutcome.DELIVERY_INSPECTED,
        models.DeliveryOutcome.RESPONSE_CAPTURED,
    }:
        return observed
    return None
