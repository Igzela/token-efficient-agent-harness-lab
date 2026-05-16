"""Error types for Event Store and JSONL validation."""


class EventStoreError(Exception):
    """Base class for Event Store errors."""


class MissingNewlineError(EventStoreError):
    """Raised when an existing JSONL line is not newline-terminated."""


class InvalidJsonLineError(EventStoreError):
    """Raised when a JSONL line is not exactly one valid JSON object."""


class DuplicateEventIdError(EventStoreError):
    """Raised when an event_id already exists in an Event Store."""


class DuplicateIdempotencyConflictError(EventStoreError):
    """Raised when an idempotency_key is reused with different semantics."""


class SchemaViolationError(EventStoreError):
    """Raised when an event does not satisfy the minimal event.v1 schema."""


class ReplayPreflightError(EventStoreError):
    """Raised when replay preflight finds blocking validation issues."""
