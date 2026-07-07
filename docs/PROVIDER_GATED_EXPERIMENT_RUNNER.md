# Provider-Gated Experiment Runner Policy

This document records the approved path for moving beyond deterministic token-efficiency pilots into real provider-backed stateful-vs-stateless experimentation.

## Decision

High-risk provider-backed experiment-runner work is allowed in this repository when it is repo-scoped, explicitly gated, CI-gated, observable, budgeted, killable, and rollbackable.

This approval supersedes any older interpretation that provider-backed real experiment runners are categorically out of scope. The remaining restriction is not "do not build it"; the restriction is "do not build it without the gates, evidence, rollback, and operator controls listed here."

## Intended Next Capability

The next implementation track may build a native real experiment runner that tests the "Remember, Don't Re-read" pattern with:

- a stateless reread mode that carries growing history context;
- a stateful store mode that keeps complete experiment history in app-owned durable state while exposing only compact summary plus a bounded recent window to the model;
- identical task, budget, iteration count, quality method, and pass criterion across both modes;
- token-efficiency scorecards for both modes;
- read-only comparison evidence including total tokens, repeated context ratio, duration, cost, status, quality method, and tokens/cost per passing run.

This is not an AR-7 phase and must not create a second Agent Runtime, scheduler, DAG kernel, mailbox, or storage layer.

## Required Gates

Any provider-backed real experiment runner must fail closed unless all required gates are present and validated:

- protected authentication for live execution;
- explicit provider execution opt-in through the trusted-local profile or documented legacy gate;
- symbolic provider access variable names only;
- provider/model identity validation;
- per-run and daily cost caps;
- token, call, timeout, iteration, and concurrency ceilings;
- operator-visible pause and kill switch;
- redaction before persistence;
- audit events for start, provider call attempt/result, iteration result, budget stop, kill stop, and finalization;
- app-owned artifact output only;
- rollback path documented in the PR body.

Provider calls must remain disabled in CI. CI tests must use stub/mock providers or deterministic fixtures.

## Persistence Boundary

Allowed persistence:

- bounded counters and metadata;
- compact experiment summaries;
- best-score/current-best metadata;
- safe state references and artifact identifiers;
- token/cost/duration/tool/retry/quality metrics;
- redacted scorecard artifacts.

Forbidden persistence:

- full model input text;
- full model output text;
- complete conversation logs;
- provider access values;
- repository full text;
- private machine paths;
- unbounded message history.

## Implementation Shape

Prefer extending existing modules:

- `engine/src/workflow/` for run/node lifecycle and bounded loop state;
- `engine/src/node_executor.rs` for executor integration;
- `engine/src/provider/` for provider calls behind existing gates;
- `engine/src/storage/local_product_store/` for app-owned bounded state and artifacts;
- `scripts/token_efficiency_scorecard.py` and scorecard helpers for validation/comparison;
- `dashboard/` only after backend evidence exists.

Do not introduce LangGraph, CrewAI, or Microsoft Agent Framework as core dependencies. External runtimes remain benchmark and trace-ingest targets unless a later documented replacement explicitly changes the architecture.

## Minimum Acceptance for the First Real Runner PR

The first provider-gated real experiment-runner PR must include:

- stub-provider tests for stateless and stateful mode;
- fail-closed tests for missing auth, missing provider gate, missing access variables, missing cost caps, kill switch, and exceeded budget;
- tests proving stateful mode uses less total context/repeated context under the same pass criterion;
- scorecard validation for both modes;
- no provider calls in CI;
- operator-visible evidence or a clearly documented follow-up if UI is deferred;
- PR body sections for gates, storage/API changes, authority changes, validation, residual risk, and rollback.

## Non-Goals

This policy does not authorize:

- direct target working-tree writes;
- protected-branch writes;
- automatic merge, deploy, release, or production publication;
- cloud or multi-tenant execution;
- provider calls in CI;
- unbounded recursive planning;
- hidden background loops without operator pause/kill evidence;
- full model input/output text persistence.
