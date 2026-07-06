# Benchmarks

This directory contains reproducible benchmark scaffolding for token-efficiency experiments.

It is intentionally not an external runtime implementation directory yet. The first goal is to keep scenarios stable while the repository defines a normalized token-efficiency scorecard.

## Layout

```text
benchmarks/
  scenarios/   reusable scenario definitions
  results/     curated, redacted, version-pinned summaries only
```

Raw high-volume traces should live in app-owned local artifacts or LocalProductStore-backed records, not in this directory.

## Current Scope

The first target is a stateful-versus-stateless comparison:

```text
native_control_plane
stateless_reread
stateful_store
```

LangGraph is the first external runtime benchmark target. CrewAI and Microsoft Agent Framework should wait until the scorecard importer and native scorecard export are stable.

## Rules

- Do not commit raw prompts, raw model outputs, transcripts, credentials, or secret-shaped values.
- Do not use benchmark code to add provider execution authority.
- Do not call paid providers in CI.
- Do not count token reduction as success unless the task also passes.
- Keep scenario definitions small, deterministic, and reviewable.
