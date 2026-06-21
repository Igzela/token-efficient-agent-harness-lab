# AF-6 Controlled Auto Fusion Proposal

Status: proposal only. This document does not authorize implementation or runtime authority.

## User-requested capability

The requested direction is to make Adaptive Fusion feel closer to a transparent fusion router:

- automatically generate candidate provider/portfolio combinations
- call multiple panel providers in parallel
- run continuous experiments
- convert live execution outcomes into observations
- promote better combinations automatically
- route live traffic by default
- allow adaptive strategy changes without human confirmation
- expose the behavior behind a normal completion-style API

## Boundary judgment

The current repository does not allow the request exactly as written. The following items conflict with the existing product boundary and should remain disallowed unless a new explicit approval changes the threat model:

- default-on provider API execution
- automatic live routing for ordinary dispatches
- unattended autonomous experimentation without hard traffic and budget limits
- automatic promotion to live behavior without approval or rollback evidence
- provider failover outside bounded AF-3 execution plans
- opaque completion behavior that hides provider/cost/audit evidence

The safe version is not "full-auto live routing". The safe version is a controlled online learning loop where automation is allowed to propose, test, score, and shadow-promote candidates, while live influence remains explicit, reversible, audited, and gated.

## AF-6 goal

Add an opt-in controlled online Adaptive Fusion loop that can learn from real adaptive_provider ticks and recommend better candidate plans by task class/objective, without granting default live routing or unmanaged provider spend.

## Approved AF-6 scope

AF-6 may implement:

1. Candidate generation
   - Deterministically generate bounded candidate portfolios from configured endpoints.
   - Candidate types: single, ordered fallback, and fusion panel/judge/synthesizer.
   - Hard caps: max 8 endpoints, max 32 generated candidates per task class/objective, max panel size 3.
   - Reject candidates that exceed cost, token, time, model-binding, capability, health, or credential-reference constraints.

2. Parallel panel execution
   - Add a new opt-in AF-6 execution mode for parallel panel calls.
   - Keep judge and synthesizer serial after panel completion.
   - Hard caps: max panel concurrency 3, max total calls 8, max total tokens/cost/time per request.
   - Preserve kill switch, circuit breakers, redaction, capped outputs, audit events, and provider identity validation.

3. Online observation ingestion
   - Convert successful or failed adaptive_provider executions into bounded observations.
   - Store observation summaries only: task_class, objective, candidate_id, success, quality score source, tool score, cost, latency, provider evidence IDs.
   - Do not store raw prompts, raw provider output, secrets, target repo content, or full panel responses.

4. Continuous shadow experimentation
   - Allow low/medium-risk traffic to assign a small percentage of explicit adaptive_provider ticks to generated candidate plans.
   - Default: disabled.
   - Required gates: ACP_ENABLE_ADAPTIVE_ONLINE_EXPERIMENTS=1 and ACP_ADAPTIVE_ONLINE_EXPERIMENTS_ACTIVE=1.
   - Hard cap: 5% traffic, no high/critical risk, no target-output/write nodes, kill switch required.

5. Auto-proposal, not auto-promotion
   - Automatically create promotion candidates when minimum sample/confidence/regression criteria are met.
   - The default output is a pending policy proposal or shadow policy snapshot.
   - Live promotion still requires explicit operator confirmation unless a later approved phase changes this.

6. Completion-style wrapper
   - Add an explicit endpoint such as POST /api/v1/adaptive-fusion/completions.
   - It may feel like a normal completion endpoint but must expose audit/cost/candidate metadata in the response.
   - It must be default-off and require auth plus dispatch:execute.
   - It must not silently bypass workflow ticks, cost gates, or audit.

## Explicitly not approved in AF-6

AF-6 must not implement:

- default-on live routing
- unbounded provider failover
- automatic live promotion without confirmation
- unattended production traffic optimization
- hidden router behavior behind /api/v1/dispatch
- any provider call without auth, budget, token, timeout, audit, redaction, and kill-switch gates
- recursive self-modifying routing code

## Implementation slices

### AF-6A Candidate generator

Files likely touched:

- engine/src/feedback/
- engine/src/provider/adaptive_execution.rs
- engine/tests/test_adaptive_fusion_*.rs

Acceptance:

- deterministic candidate IDs and hashes
- rejects invalid/over-budget candidates
- no network/provider calls
- tests cover duplicate, malformed, over-budget, capability-missing, and secret-shaped cases

### AF-6B Parallel panel execution

Files likely touched:

- engine/src/provider/adaptive_execution.rs
- engine/tests/test_adaptive_fusion_execution.rs

Acceptance:

- panel calls may run concurrently only under explicit AF-6 gate
- max concurrency <= 3
- judge/synthesizer remain serial
- kill switch cancels subsequent stages and blocks new calls
- failure behavior is deterministic and audited

### AF-6C Online observation capture

Files likely touched:

- engine/src/storage/local_product_store/
- engine/src/feedback/contextual_policy.rs
- engine/src/http_server/handlers/workflow_runs.rs

Acceptance:

- adaptive tick results generate bounded observation summaries
- raw prompts/outputs are not persisted
- observations can feed existing offline/contextual scoring
- redaction and size caps are tested

### AF-6D Shadow auto-proposals

Files likely touched:

- engine/src/storage/local_product_store/adaptive_policy.rs
- engine/src/http_server/handlers/dispatch.rs
- dashboard/src/components/AdaptiveFusion*.tsx
- sdk/typescript/src/*

Acceptance:

- eligible evidence can generate pending promotion proposals
- live promotion remains confirmation-gated
- rollback remains available
- dashboard distinguishes shadow proposal from active policy

### AF-6E Explicit completion-style endpoint

Files likely touched:

- engine/src/http_server/routes.rs
- engine/src/http_server/handlers/
- sdk/typescript/src/*
- dashboard/src/lib/api-client.ts

Acceptance:

- explicit endpoint, not default /dispatch behavior
- requires auth + dispatch:execute
- provider execution and adaptive gates required
- response includes candidate/audit/cost metadata
- no hidden provider spend

## Merge policy

Do not auto-merge AF-6 implementation PRs. Each slice touches provider routing authority and requires explicit review after CI.

Required verification per slice:

```bash
cargo fmt --all -- --check
cargo clippy -p engine --all-targets -- -D warnings
cargo test -p engine --test test_adaptive_fusion_execution
cargo test -p engine --test test_contextual_adaptive_policy
bash scripts/verify_rust_typescript_stack.sh
uv run --no-project python scripts/check_agent_handoff.py
git diff --check
```

## Final acceptance for AF-6

AF-6 is complete only when the system can:

- generate bounded candidates from configured endpoints
- run opt-in parallel panel fusion with audited calls
- convert adaptive_provider outcomes into safe observations
- continuously experiment only under explicit gates and hard caps
- create shadow promotion proposals automatically
- keep live promotion explicit, reversible, and audited
- expose an explicit completion-style API without hiding cost/audit/routing metadata

