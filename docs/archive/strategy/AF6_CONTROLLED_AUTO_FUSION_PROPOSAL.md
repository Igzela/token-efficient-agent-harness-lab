# AF-6 Auto Fusion Boundary Reset

Status: authorized planning baseline after user direction on 2026-06-21.

This document replaces the earlier conservative AF-6 proposal. The older repository boundary treated default live routing, provider failover, unattended experiments, and automatic promotion as disallowed by default. For AF-6, those items are no longer categorically disallowed. They are now target capabilities, provided they are implemented through staged gates, hard budgets, audit, kill switches, rollback, and CI-backed verification.

## User-requested capability

AF-6 should make Adaptive Fusion feel closer to a transparent fusion router:

- automatically generate candidate provider/portfolio combinations
- call multiple panel providers in parallel
- run continuous experiments
- convert live execution outcomes into observations
- promote better combinations automatically
- route live traffic by default after explicit AF-6 enablement
- allow adaptive strategy changes without per-promotion human confirmation
- expose the behavior behind a normal completion-style API

## Boundary reset

AF-6 changes the old boundary from "do not implement these capabilities" to "implement these capabilities with explicit operational controls".

The following are now in scope for AF-6:

- live adaptive routing for ordinary completion-style requests
- provider fallback and fusion selection across configured endpoints
- automatic candidate generation
- automatic online experiments
- automatic observation capture
- automatic policy promotion after evidence thresholds
- a completion-style API wrapper that can hide orchestration complexity from callers

The following remain non-negotiable engineering controls, not product-boundary exclusions:

- auth for every live provider call
- per-request and daily cost gates
- token, call, timeout, and concurrency ceilings
- provider/model identity validation
- secret redaction and output caps
- circuit breakers and global kill switches
- persistent audit events and policy snapshots
- rollback path for every promoted policy
- tests and CI before merge

## AF-6 target behavior

AF-6 should support this flow:

```text
completion request
-> classify task context/objective/risk
-> load configured provider endpoints
-> generate eligible single/fallback/fusion candidates
-> select candidate from active adaptive policy or exploration policy
-> execute candidate, including parallel panel calls when selected
-> judge/synthesize when using fusion
-> return final output plus optional routing metadata
-> persist observation summary
-> update evidence aggregates
-> promote better policy automatically when thresholds are met
-> rollback or kill if gates trip
```

## Required implementation slices

### AF-6A Candidate generator

Goal: deterministically generate candidate portfolios from configured endpoints.

Scope:

- single endpoint candidates
- ordered fallback candidates
- fusion candidates with panel, judge, and synthesizer roles
- objective profiles such as efficient, quality, balanced, and low-latency if useful
- health, context, tool, capability, price, model-binding, and credential-reference filters

Acceptance:

- generated candidate IDs and hashes are deterministic
- no provider call occurs during generation
- max endpoints and max candidates are bounded by config
- invalid, duplicate, over-budget, secret-shaped, unavailable, or capability-missing candidates are rejected
- tests cover deterministic ordering and rejection cases

### AF-6B Parallel panel execution

Goal: allow Fusion panel calls to run concurrently.

Scope:

- parallelize only panel calls
- keep judge and synthesizer after panel completion
- support partial panel failure policy, e.g. require at least one or at least two successful panel outputs depending on risk/objective
- preserve timeout, cost, token, identity, redaction, audit, and kill switch behavior

Acceptance:

- max panel concurrency is bounded and configurable
- max total calls, total tokens, total cost, and total elapsed time still apply
- kill switch stops new calls and prevents judge/synthesizer after cancellation
- partial failure behavior is deterministic and audited
- tests cover all-success, partial-failure, timeout, kill, identity mismatch, and cost/token overrun

### AF-6C Online observation capture

Goal: convert real adaptive executions into bounded learning data.

Scope:

- capture candidate_id, task_class, objective, risk level, success, tool success, quality score source, cost, latency, token usage, provider evidence IDs, and policy hash
- support observations from single, fallback, and fusion executions
- avoid raw prompt, raw output, secret, target repo content, and full provider transcript persistence

Acceptance:

- observation summaries are persisted in the existing local store or an approved migration
- raw sensitive material is not stored
- observations can feed existing offline/contextual scoring
- malformed or oversized observations are rejected
- tests cover redaction, caps, duplicate evidence, and unknown candidate handling

### AF-6D Continuous experiments

Goal: let the system try candidate combinations continuously under controlled traffic allocation.

Scope:

- exploration percentage configurable by environment and policy
- default experiment traffic can be enabled by AF-6 config
- risk-aware exploration limits remain available
- experiments should include cheap/fast candidates and quality/fusion candidates

Acceptance:

- traffic allocation is deterministic for a request_id/seed
- experiments cannot exceed configured request, token, cost, and concurrency caps
- experiments can be paused or killed globally
- active experiment policy and recent outcomes are visible through API/dashboard

### AF-6E Automatic promotion

Goal: promote better provider combinations automatically after evidence thresholds are met.

Scope:

- remove per-promotion human confirmation as a hard requirement for AF-6 auto mode
- require minimum samples, confidence, quality delta, cost delta, failure-rate guard, and fresh evidence
- support staged rollout percentages before full live routing
- persist every promotion as a policy snapshot with rollback metadata

Acceptance:

- automatic promotion requires explicit AF-6 auto-promotion enablement
- promoted policies carry policy hashes and evidence IDs
- rollback can restore the previous active policy
- promotion is blocked by regression, insufficient evidence, missing local evidence, or kill switch
- tests cover eligible promotion, blocked promotion, rollback, and staged rollout

### AF-6F Default adaptive live routing

Goal: make adaptive routing the default path for completion-style requests after AF-6 is enabled.

Scope:

- add a completion-style endpoint such as `POST /api/v1/adaptive-fusion/completions`
- optionally allow `/api/v1/dispatch` to delegate to adaptive routing only when an explicit AF-6 default-live gate is enabled
- return normal completion output while exposing optional routing metadata for operators

Acceptance:

- live routing is not active unless AF-6 default-live config is set
- provider execution, auth, cost, token, timeout, audit, redaction, and kill switch controls remain enforced
- callers can request compact output, while operators can inspect routing/cost/audit details
- tests cover default-off, enabled, unauthorized, over-budget, killed, and successful paths

## Environment gates

AF-6 may introduce gates like:

```text
ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION=1
ACP_ENABLE_ADAPTIVE_AUTO_ROUTING=1
ACP_ADAPTIVE_AUTO_ROUTING_ACTIVE=1
ACP_ENABLE_ADAPTIVE_ONLINE_EXPERIMENTS=1
ACP_ADAPTIVE_ONLINE_EXPERIMENTS_ACTIVE=1
ACP_ENABLE_ADAPTIVE_AUTO_PROMOTION=1
ACP_ADAPTIVE_AUTO_PROMOTION_ACTIVE=1
ACP_ADAPTIVE_DEFAULT_LIVE_ROUTING=1
ACP_ADAPTIVE_FUSION_KILL_SWITCH=1
ACP_ADAPTIVE_AUTO_PROMOTION_KILL_SWITCH=1
```

Exact names may change during implementation, but equivalent controls must exist.

## Explicitly still out of scope

AF-6 still must not implement:

- provider calls without auth in configured-auth mode
- unbounded spend or unbounded concurrency
- storage of raw secrets, raw provider transcripts, or target repository content as learning data
- recursive self-modifying code
- release/deploy/merge authority from the adaptive router
- bypassing CI, tests, or audit

## Merge policy

AF-6 implementation PRs are not auto-merge eligible. They touch provider routing authority and require explicit review after CI.

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

## Final AF-6 acceptance

AF-6 is complete only when the system can:

- generate bounded candidates from configured endpoints
- run parallel panel fusion with audited calls
- convert live outcomes into safe observations
- continuously experiment under explicit AF-6 controls
- automatically promote better policies after evidence thresholds
- route completion-style requests through adaptive live routing when enabled
- preserve cost, token, auth, audit, redaction, rollback, and kill-switch controls
