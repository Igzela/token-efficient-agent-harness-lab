# LangGraph Stateful vs Stateless Benchmark Plan

## Purpose

This plan defines the first external-runtime benchmark target for the token-efficiency scorecard.

The goal is not to adopt LangGraph as the internal runtime. The goal is to compare context-control strategies under a stable scenario and a normalized scorecard.

## Hypothesis

For iterative agent tasks, a stateless reread loop repeatedly injects growing history into each model step. A stateful workflow stores durable state and injects only the current task state, relevant memory digest, and retrieval references. As iteration count increases, the stateless mode should show a steeper context-token curve than the stateful mode.

## Modes

The first benchmark should compare three modes:

```text
native_control_plane
stateless_reread
stateful_store
```

A later benchmark may add:

```text
pruned_context
stateful_store_plus_retrieval_refs
```

## First Scenario

Use `benchmarks/scenarios/iterative_debug_basic.json` as the first scenario because iterative debugging naturally creates repeated history, retries, tool calls, and evaluation checkpoints.

The scenario must be executable without secrets and without real provider calls in CI. Real-provider local trials may be run only under the existing trusted-local/provider gates and should record that fact in the scorecard.

## Measurement Requirements

Each mode must produce a run-level scorecard and step-level summaries:

- pass/fail status;
- quality method;
- total input and output tokens;
- context tokens per step;
- repeated context token estimate;
- tool call count;
- redundant tool call count;
- retry count;
- duration;
- raw trace artifact reference;
- redaction status.

## Acceptance Gates

The benchmark is valid only if:

- every mode runs the same scenario;
- every mode uses the same success criterion;
- token counts or estimates use the same tokenizer or clearly record the estimate method;
- raw traces are redacted, bounded, and artifact-backed;
- no provider calls run in CI;
- lower token use is not counted as success unless the task also passes.

## Implementation Sequence

1. Keep this document and the scenario files as the planning baseline.
2. Add an importer that accepts a bounded trace summary and emits `token_efficiency_scorecard.v1`.
3. Add native harness scorecard export.
4. Add a minimal LangGraph trace importer.
5. Add the stateless reread baseline.
6. Add the stateful-store baseline.
7. Only after those are stable, consider CrewAI or Microsoft Agent Framework baselines.

## Non-Goals

- Do not make LangGraph a required engine dependency.
- Do not replace the existing scheduler, workflow graph, node executor, or LocalProductStore.
- Do not add target-output, merge, deploy, or release authority.
- Do not persist raw prompts, raw model outputs, transcripts, secrets, or unbounded tool output.
- Do not claim benchmark superiority without pass/fail and quality evidence.
