# Agent Instructions

This project is implementing Token-Efficient Agent Harness.

Current phase:
- Stage 1 Day 1 only.
- Implement Event Store + JSONL Validator + minimal Kernel event append contract.

Hard rules:
- Do not modify docs/stage0/events.jsonl. It contains a known bad line 17 and must remain unchanged as a validator fixture.
- Do not implement all Stage 1 components.
- Do not build Web UI.
- Do not build model calls, provider failover, routing optimizer, skill extractor, dynamic DAG mutation, fragment integrator, real multi-agent concurrency, or build sampling.
- Prefer Python stdlib unless an existing project stack clearly dictates otherwise.
- Do not install dependencies without explicit approval.
- Do not commit git changes unless explicitly instructed.
- Keep changes small and reviewable.

Stage 1 Day 1 scope:
- Event Store
- JSONL Validator
- Event schema validation
- event_id uniqueness
- idempotency_key behavior
- replay preflight check
- tests for the Stage 0 line 17 issue

Event semantics:
- event_id is globally unique. Duplicate event_id must be rejected.
- idempotency_key may repeat.
- Same idempotency_key + same payload hash => duplicate no-op.
- Same idempotency_key + different payload hash => reject with conflict.
- JSONL must be one JSON object per line.
- Every appended line must end with newline.
- Original docs/stage0/events.jsonl must not be fixed.
