# Runtime Adapter Boundary

## Status

Accepted boundary after merge of this document. This is a docs-only architecture clarification; it does not add runtime authority, database migrations, provider calls, target-output authority, release authority, or a second scheduler.

## Decision

`token-efficient-agent-harness-lab` treats LangGraph, CrewAI, Microsoft Agent Framework, and similar systems as external runtime benchmark or trace-ingest targets. They are not core dependencies, not replacement runtimes, and not new kernels inside this repository.

The harness remains the meta-harness, regulator, evaluator, and evidence layer above or beside agent runtimes. Its job is to measure and control whether an agent workflow uses fewer tokens, fewer repeated reads, fewer redundant tool calls, and fewer ineffective loops while preserving task success and reviewable evidence.

## Rationale

External agent runtimes answer a different question:

```text
How do agents, tools, state, memory, and workflow execution run?
```

This repository answers the token-efficiency control question:

```text
For the same task, how much context was read, reread, injected, pruned, retrieved, or wasted, and did the run still pass?
```

LangGraph is the most useful first benchmark target because its long-running stateful workflow model makes it suitable for stateful-versus-stateless context cost experiments. CrewAI is useful as a high-level application baseline. Microsoft Agent Framework is useful as a telemetry, middleware, and evaluation reference. None of them should drive the internal architecture toward a parallel workflow kernel.

## Boundary

Allowed adapter work:

- ingest external runtime traces into app-owned artifacts;
- normalize external run and step metrics into a token-efficiency scorecard;
- compare native harness, stateless reread, stateful store, and pruned-context runs;
- preserve source runtime identity, version, scenario, mode, and trace references;
- display summarized scorecards in operator evidence surfaces;
- keep raw traces bounded, redacted, and artifact-backed.

Not allowed without a later explicit decision:

- replacing `workflow_runs`, `scheduler`, `node_executor`, `provider`, `cli`, or `LocalProductStore` with an external runtime;
- adding a second scheduler, DAG engine, storage layer, hidden mailbox, or side-channel state system;
- making LangGraph, CrewAI, or Microsoft Agent Framework mandatory runtime dependencies for the core engine;
- persisting raw prompts, raw model outputs, transcripts, secrets, or unbounded context history;
- granting provider execution, target-output, merge, deploy, release, or protected-branch authority through an adapter;
- calling paid providers in CI.

## Record Placement

Adapter records have three layers.

### 1. Static definition

Static architecture and benchmark boundaries live in repository docs:

- `docs/benchmarks/runtime-adapter-boundary.md`
- `docs/benchmarks/token-efficiency-scorecard.md`
- benchmark plans under `docs/benchmarks/`

Dynamic adapter run state must not be written into GPT Project source files.

### 2. Runtime records

A later implementation may persist normalized records in `LocalProductStore` and app-owned artifacts. The intended logical entities are:

```text
runtime_adapter_runs
runtime_adapter_steps
runtime_adapter_artifacts
```

These names are a design contract, not an implemented schema in this docs-only change.

### 3. Reproducible benchmark material

Reusable benchmark scenarios and curated summaries live under:

```text
benchmarks/scenarios/
benchmarks/results/
```

Raw high-volume traces should remain in app-owned local artifacts unless a curated, redacted, version-pinned summary is intentionally committed.

## First Implementation Target

The first implementation target should be an ingest-only scorecard path, not a full external runtime runner:

1. define a scenario;
2. run native or external runtime manually or through a bounded script;
3. import bounded trace metadata;
4. compute the token-efficiency scorecard;
5. compare modes.

The first external baseline should be LangGraph stateful versus stateless reread. CrewAI and Microsoft Agent Framework adapters should wait until the scorecard shape is stable.

## Validation Standard

A runtime adapter change passes only if it proves:

- no new execution authority was added;
- no parallel runtime kernel was introduced;
- raw traces are bounded and redacted;
- normalized metrics are reproducible from stored evidence;
- benchmark scenarios are versioned;
- pass/fail quality is recorded separately from token savings;
- token savings are not reported as success when the task failed.
