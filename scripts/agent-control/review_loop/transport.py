"""Transport interface and CI-safe fake.

The repository owns the protocol contract; the actual browser/login side is
an operator-local adapter that implements this interface and is never part of
CI.  A transport must never decide whether to resend, what PASS means, or
whether to post a comment.
"""

from __future__ import annotations

import dataclasses
import typing

# Closed set of thread-inspection states (R2-B2).  Anything outside this set
# must fail closed, never fall through to a send.
VALID_INSPECTION_STATES = frozenset({"EMPTY_THREAD", "MESSAGE", "INSPECTION_UNAVAILABLE"})


@dataclasses.dataclass(frozen=True)
class ThreadInspection:
    """Three-state result of inspecting the thread before a send.

    - EMPTY_THREAD: provably no prior user message (first send allowed).
    - MESSAGE: the thread's newest user message text is known (text required).
    - INSPECTION_UNAVAILABLE: the transport could not prove the state
      (page not loaded, selector failure, login issue).  A caller must stop,
      never treat this as "no message".

    ``state`` is a closed enum: ``VALID_INSPECTION_STATES``.  Any other value
    is a transport bug and must fail closed (R2-B2).
    """

    state: str
    text: str | None = None

    def __post_init__(self):
        if self.state not in VALID_INSPECTION_STATES:
            raise ValueError(
                f"invalid ThreadInspection state {self.state!r}; "
                f"must be one of {sorted(VALID_INSPECTION_STATES)}"
            )
        if self.state == "MESSAGE" and not self.text:
            raise ValueError("MESSAGE inspection requires the message text")

    @classmethod
    def empty(cls) -> "ThreadInspection":
        return cls("EMPTY_THREAD")

    @classmethod
    def message(cls, text: str) -> "ThreadInspection":
        return cls("MESSAGE", text)

    @classmethod
    def unavailable(cls, reason: str = "") -> "ThreadInspection":
        return cls("INSPECTION_UNAVAILABLE", reason)


class Transport(typing.Protocol):
    """Minimal browser transport surface (operator-local implementation)."""

    def read_auth_state(self) -> bool:
        """Return True when the session is authenticated, False when logged out."""
        ...

    def inspect_last_user_message(self) -> ThreadInspection:
        """Inspect the thread's newest user message (three-state)."""
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
        inspect_state: str = "auto",
    ):
        self.authed = authed
        self.user_messages: list[str] = list(user_messages or [])
        self.assistant_replies: list[str] = list(assistant_replies or [])
        self.sent_calls: list[str] = []
        self.inspect_state = inspect_state  # auto | unavailable
        self.send_failure: BaseException | None = None

    def read_auth_state(self) -> bool:
        return self.authed

    def inspect_last_user_message(self) -> ThreadInspection:
        if self.inspect_state == "unavailable":
            return ThreadInspection.unavailable("scripted inspection failure")
        if not self.user_messages:
            return ThreadInspection.empty()
        return ThreadInspection.message(self.user_messages[-1])

    def send_user_message(self, text: str) -> None:
        if self.send_failure is not None:
            raise self.send_failure
        self.sent_calls.append(text)
        self.user_messages.append(text)

    def read_latest_assistant_message(self) -> str | None:
        return self.assistant_replies[-1] if self.assistant_replies else None
