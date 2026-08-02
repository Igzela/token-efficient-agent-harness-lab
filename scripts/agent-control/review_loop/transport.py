"""Transport interface and CI-safe fake.

The repository owns the protocol contract; the actual browser/login side is
an operator-local adapter that implements this interface and is never part of
CI.  A transport must never decide whether to resend, what PASS means, or
whether to post a comment.
"""

from __future__ import annotations

import typing


class Transport(typing.Protocol):
    """Minimal browser transport surface (operator-local implementation)."""

    def read_auth_state(self) -> bool:
        """Return True when the session is authenticated, False when logged out."""
        ...

    def read_last_user_message(self) -> str | None:
        """Return the text of the thread's newest user message, or None."""
        ...

    def send_user_message(self, text: str) -> None:
        """Deliver one message.  Must not silently resend identical text."""
        ...

    def read_latest_assistant_message(self) -> str | None:
        """Return the thread's newest assistant reply text, or None."""
        ...


class FakeTransport:
    """Deterministic in-memory transport for provider-free tests.

    Holds a scripted sequence of user messages and assistant replies; records
    every send call so tests can assert idempotency.
    """

    def __init__(
        self,
        *,
        authed: bool = True,
        user_messages: list[str] | None = None,
        assistant_replies: list[str] | None = None,
    ):
        self.authed = authed
        self.user_messages: list[str] = list(user_messages or [])
        self.assistant_replies: list[str] = list(assistant_replies or [])
        self.sent_calls: list[str] = []

    def read_auth_state(self) -> bool:
        return self.authed

    def read_last_user_message(self) -> str | None:
        return self.user_messages[-1] if self.user_messages else None

    def send_user_message(self, text: str) -> None:
        self.sent_calls.append(text)
        self.user_messages.append(text)

    def read_latest_assistant_message(self) -> str | None:
        return self.assistant_replies[-1] if self.assistant_replies else None
